"""
Build ScreenPipe Knowledge Graph dashboard in Apache Superset.

Creates 3 virtual SQL datasets and 8 charts:
  - KG overview stats (big numbers)
  - Entity type distribution (pie)
  - Top entities (table)
  - Knowledge graph flow (Sankey: relation → entity type)
  - Entity activity over time (stacked bar)
  - Top visited URLs (table)
  - Top commands run (bar)
  - Work sessions (table)
"""
import json, requests, sys

BASE = "http://localhost:8088"
DB_ID = 1  # ScreenPipe PostgreSQL connection

# ── Auth ──────────────────────────────────────────────────────────────────────
s = requests.Session()
token = s.post(f"{BASE}/api/v1/security/login",
    json={"username": "admin", "password": "admin",
          "provider": "db", "refresh": False}
).json()["access_token"]
s.headers.update({"Authorization": f"Bearer {token}"})
csrf = s.get(f"{BASE}/api/v1/security/csrf_token/").json()["result"]
s.headers.update({"X-CSRFToken": csrf, "Content-Type": "application/json"})


def mk_dataset(name, sql):
    r = s.post(f"{BASE}/api/v1/dataset/", json={
        "database": DB_ID,
        "schema": "public",
        "table_name": name,
        "sql": sql,
    })
    d = r.json()
    ds_id = d.get("id")
    if not ds_id:
        print(f"  ERR dataset '{name}': {d}")
        return None
    # Trigger column refresh so Superset knows the schema
    s.put(f"{BASE}/api/v1/dataset/{ds_id}/refresh")
    print(f"  Dataset [{ds_id}] {name}")
    return ds_id


def chart(ds_id, name, viz_type, params):
    body = {
        "slice_name": name,
        "viz_type": viz_type,
        "datasource_id": ds_id,
        "datasource_type": "table",
        "params": json.dumps({
            **params,
            "viz_type": viz_type,
            "datasource": f"{ds_id}__table",
        }),
    }
    r = s.post(f"{BASE}/api/v1/chart/", json=body)
    d = r.json()
    cid = d.get("id")
    print(f"  {'OK' if cid else 'ERR'} [{cid}] {name}" +
          ("" if cid else f" — {d}"))
    return cid


# ── Virtual datasets ───────────────────────────────────────────────────────────
print("Creating KG datasets...")

# Dataset 1: per-edge occurrence rows with app, relation, entity info + timestamp
ds_occ = mk_dataset("kg_entity_occurrences", """
SELECT
    e.id          AS edge_id,
    e.created_at,
    e.relation,
    n.node_type,
    n.value       AS entity_value,
    LEFT(n.value, 60) AS entity_short,
    f.app_name,
    f.captured_at,
    f.session_id
FROM kg_edges e
JOIN kg_nodes n ON n.id = e.dst_node_id
JOIN frames   f ON f.id = e.frame_id
""")

# Dataset 2: aggregated source→target flows for Sankey
# Shows: which app/relation produces which entity type
ds_flows = mk_dataset("kg_edge_flows", """
SELECT
    e.relation                    AS source,
    n.node_type                   AS target,
    COUNT(*)                      AS edge_count
FROM kg_edges e
JOIN kg_nodes n ON n.id = e.dst_node_id
GROUP BY e.relation, n.node_type
""")

# Dataset 3: sessions enriched with entity counts
ds_sess = mk_dataset("kg_sessions_enriched", """
SELECT
    s.id,
    s.started_at,
    s.ended_at,
    s.frame_count,
    ROUND(EXTRACT(EPOCH FROM (s.ended_at - s.started_at)) / 60.0, 1)
        AS duration_mins,
    COUNT(DISTINCT e.dst_node_id) AS unique_entities,
    COUNT(DISTINCT e.id)          AS total_edges
FROM kg_sessions s
LEFT JOIN frames   f ON f.session_id = s.id
LEFT JOIN kg_edges e ON e.frame_id   = f.id
GROUP BY s.id, s.started_at, s.ended_at, s.frame_count
ORDER BY s.started_at DESC
""")

if not (ds_occ and ds_flows):
    print("ERROR: dataset creation failed — aborting", file=sys.stderr)
    sys.exit(1)

# ── Charts ────────────────────────────────────────────────────────────────────
print("\nCreating KG charts...")

COUNT_EDGES = {
    "aggregate": "COUNT",
    "column": {"column_name": "edge_id"},
    "expressionType": "SIMPLE",
    "label": "Edges",
}

# 1. Total unique entities (big number)
c1 = chart(ds_occ, "Unique Entities Extracted", "big_number_total", {
    "metric": {
        "expressionType": "SQL",
        "sqlExpression": "COUNT(DISTINCT entity_value)",
        "label": "Unique Entities",
    },
    "time_range": "No filter",
    "subheader": "unique entities in knowledge graph",
    "y_axis_format": "SMART_NUMBER",
})

# 2. Total KG edges (big number)
c2 = chart(ds_occ, "Total KG Edges", "big_number_total", {
    "metric": COUNT_EDGES,
    "time_range": "No filter",
    "subheader": "knowledge graph connections",
    "y_axis_format": "SMART_NUMBER",
})

# 3. Entity type distribution (pie — what types of entities are being extracted)
c3 = chart(ds_occ, "Entity Type Distribution", "pie", {
    "groupby": ["node_type"],
    "metric": COUNT_EDGES,
    "time_range": "No filter",
    "donut": True,
    "show_labels": True,
    "show_legend": True,
    "label_type": "key_percent",
    "row_limit": 20,
})

# 4. Sankey: relation → entity_type (how KG edges flow)
c4 = chart(ds_flows, "Knowledge Graph Flow", "sankey_v2", {
    "source": "source",
    "target": "target",
    "metric": {
        "expressionType": "SIMPLE",
        "aggregate": "SUM",
        "column": {"column_name": "edge_count"},
        "label": "Connections",
    },
    "time_range": "No filter",
})

# 5. KG edge activity over time by relation (stacked bar, 1-min buckets)
c5 = chart(ds_occ, "Entity Extraction Activity", "echarts_timeseries_bar", {
    "x_axis": "created_at",
    "time_grain_sqla": "PT1M",
    "metrics": [COUNT_EDGES],
    "groupby": ["relation"],
    "time_range": "No filter",
    "rich_tooltip": True,
    "show_legend": True,
    "x_axis_time_format": "smart_date",
    "stack": True,
})

# 6. Top entities table (most frequently observed entities)
c6 = chart(ds_occ, "Top Entities", "table", {
    "groupby": ["node_type", "entity_short", "relation"],
    "metrics": [COUNT_EDGES],
    "time_range": "No filter",
    "row_limit": 25,
    "order_desc": True,
    "include_search": True,
    "page_length": 25,
    "column_config": {
        "node_type": {"label": "Type"},
        "entity_short": {"label": "Entity"},
        "relation": {"label": "Relation"},
        "Edges": {"label": "Occurrences"},
    },
})

# 7. App breakdown by entity type (pie — which apps generate which entity types)
c7 = chart(ds_occ, "Entity Activity by App", "pie", {
    "groupby": ["app_name"],
    "metric": COUNT_EDGES,
    "time_range": "No filter",
    "donut": False,
    "show_labels": True,
    "show_legend": True,
    "label_type": "key_percent",
    "row_limit": 10,
})

# 8. Sessions table (when sessions are assigned)
c8 = chart(ds_sess, "Work Sessions", "table", {
    "groupby": ["id", "started_at", "ended_at", "duration_mins",
                "frame_count", "unique_entities", "total_edges"],
    "metrics": [],
    "time_range": "No filter",
    "row_limit": 20,
    "order_desc": True,
    "include_search": False,
    "page_length": 10,
}) if ds_sess else None

chart_ids = [c for c in [c1, c2, c3, c4, c5, c6, c7, c8] if c]
print(f"  Created {len(chart_ids)} charts: {chart_ids}")

# ── Dashboard layout ───────────────────────────────────────────────────────────
print("\nBuilding dashboard layout...")

# Helper: make a CHART component entry
def comp(cid, chart_id, row, width, height):
    return {
        "type": "CHART", "id": cid, "children": [],
        "parents": ["ROOT_ID", "GRID_ID", row],
        "meta": {"chartId": chart_id, "width": width, "height": height},
    }

def row_comp(rid, children):
    return {
        "type": "ROW", "id": rid, "children": children,
        "parents": ["ROOT_ID", "GRID_ID"],
        "meta": {"background": "BACKGROUND_TRANSPARENT"},
    }

# Build position map — gracefully handle missing charts
ci = {i: chart_ids[i] for i in range(len(chart_ids))}

components = {
    "ROOT_ID": {"type": "ROOT", "id": "ROOT_ID", "children": ["GRID_ID"]},
    "GRID_ID": {
        "type": "GRID", "id": "GRID_ID",
        "children": ["ROW-1", "ROW-2", "ROW-3", "ROW-4"],
        "parents": ["ROOT_ID"],
    },
    # Row 1: 2 big numbers (c1, c2)
    **{k: v for k, v in {
        "ROW-1": row_comp("ROW-1", ["C1", "C2"]),
        "C1": comp("C1", ci[0], "ROW-1", 6, 100),
        "C2": comp("C2", ci[1], "ROW-1", 6, 100),
    }.items() if len(ci) > 1},
    # Row 2: entity type pie + sankey (c3, c4)
    **{k: v for k, v in {
        "ROW-2": row_comp("ROW-2", ["C3", "C4"]),
        "C3": comp("C3", ci[2], "ROW-2", 4, 380),
        "C4": comp("C4", ci[3], "ROW-2", 8, 380),
    }.items() if len(ci) > 3},
    # Row 3: activity timeseries bar + top entities table (c5, c6)
    **{k: v for k, v in {
        "ROW-3": row_comp("ROW-3", ["C5", "C6"]),
        "C5": comp("C5", ci[4], "ROW-3", 5, 380),
        "C6": comp("C6", ci[5], "ROW-3", 7, 380),
    }.items() if len(ci) > 5},
    # Row 4: app entity pie + sessions table (c7, c8)
    **{k: v for k, v in {
        "ROW-4": row_comp("ROW-4", (["C7"] + (["C8"] if len(ci) > 7 else []))),
        "C7": comp("C7", ci[6], "ROW-4", (4 if len(ci) > 7 else 12), 300),
        **({"C8": comp("C8", ci[7], "ROW-4", 8, 300)} if len(ci) > 7 else {}),
    }.items() if len(ci) > 6},
}

# ── Create dashboard ──────────────────────────────────────────────────────────
r = s.post(f"{BASE}/api/v1/dashboard/", json={
    "dashboard_title": "ScreenPipe KG",
    "slug": "screenpipe-kg",
    "position_json": json.dumps(components),
    "json_metadata": json.dumps({"refresh_frequency": 30}),
    "published": True,
})
d = r.json()
dash_id = d.get("id")
if not dash_id:
    print(f"ERROR creating dashboard: {d}", file=sys.stderr)
    sys.exit(1)
print(f"  Dashboard [{dash_id}]: ScreenPipe KG")

# Link all charts to the dashboard
for cid in chart_ids:
    s.put(f"{BASE}/api/v1/chart/{cid}", json={"dashboards": [dash_id]})

print(f"\nDone!  {BASE}/superset/dashboard/{dash_id}/")
