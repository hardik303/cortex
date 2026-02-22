"""
Rebuild ScreenPipe dashboard with correct Superset 6 viz types and params.
"""
import json, requests, sys

BASE = "http://localhost:8088"
DATASET_ID = 1
DS = f"{DATASET_ID}__table"

s = requests.Session()
token = s.post(f"{BASE}/api/v1/security/login",
    json={"username":"admin","password":"admin","provider":"db","refresh":False}
).json()["access_token"]
s.headers.update({"Authorization": f"Bearer {token}"})
csrf = s.get(f"{BASE}/api/v1/security/csrf_token/").json()["result"]
s.headers.update({"X-CSRFToken": csrf, "Content-Type": "application/json"})

def chart(name, viz_type, params):
    body = {
        "slice_name": name,
        "viz_type": viz_type,
        "datasource_id": DATASET_ID,
        "datasource_type": "table",
        "params": json.dumps({**params, "viz_type": viz_type, "datasource": DS}),
    }
    r = s.post(f"{BASE}/api/v1/chart/", json=body)
    d = r.json()
    cid = d.get("id")
    print(f"  {'OK' if cid else 'ERR'} [{cid}] {name}" + ("" if cid else f" — {d}"))
    return cid

COUNT_ID = {"aggregate": "COUNT", "column": {"column_name": "id"},
            "expressionType": "SIMPLE", "label": "Frames"}

print("Creating charts...")

# 1. Big number — total frames
c1 = chart("Total Frames Captured", "big_number_total", {
    "metric": COUNT_ID,
    "time_range": "No filter",
    "subheader": "frames recorded",
    "y_axis_format": "SMART_NUMBER",
})

# 2. Big number — avg OCR length
c2 = chart("Avg OCR Text Length", "big_number_total", {
    "metric": {
        "expressionType": "SQL",
        "sqlExpression": "AVG(LENGTH(ocr_text))",
        "label": "Avg OCR Length",
    },
    "time_range": "No filter",
    "subheader": "characters per frame",
    "y_axis_format": "SMART_NUMBER",
})

# 3. Pie — screen time by app
c3 = chart("Screen Time by App", "pie", {
    "groupby": ["app_name"],
    "metric": COUNT_ID,
    "time_range": "No filter",
    "donut": True,
    "show_labels": True,
    "show_legend": True,
    "label_type": "key_percent",
    "row_limit": 20,
})

# 4. Line — capture rate over time (echarts timeseries)
c4 = chart("Capture Rate Over Time", "echarts_timeseries_line", {
    "x_axis": "captured_at",
    "time_grain_sqla": "PT1M",
    "metrics": [COUNT_ID],
    "groupby": [],
    "time_range": "No filter",
    "rich_tooltip": True,
    "show_legend": False,
    "x_axis_time_format": "smart_date",
    "y_axis_title": "Frames / min",
})

# 5. Line — avg OCR text length over time
c5 = chart("OCR Text Length Over Time", "echarts_timeseries_line", {
    "x_axis": "captured_at",
    "time_grain_sqla": "PT1M",
    "metrics": [{
        "expressionType": "SQL",
        "sqlExpression": "AVG(LENGTH(ocr_text))",
        "label": "Avg OCR Length",
    }],
    "groupby": [],
    "time_range": "No filter",
    "rich_tooltip": True,
    "show_legend": False,
    "x_axis_time_format": "smart_date",
    "y_axis_title": "Characters",
})

# 6. Table — top window titles (most reliable for categorical data)
c6 = chart("Top Window Titles", "table", {
    "groupby": ["window_title", "app_name"],
    "metrics": [COUNT_ID],
    "time_range": "No filter",
    "row_limit": 15,
    "order_desc": True,
    "include_search": True,
    "page_length": 15,
})

chart_ids = [c for c in [c1, c2, c3, c4, c5, c6] if c]

# ── Dashboard layout ──────────────────────────────────────────────────────────
print("\nBuilding dashboard layout...")

# Superset 6 position_json schema
# Row heights: big numbers = 2 units (~100px), charts = 6 units (~300px)
# Total grid width = 12 columns

components = {
    "ROOT_ID": {
        "type": "ROOT", "id": "ROOT_ID",
        "children": ["GRID_ID"],
    },
    "GRID_ID": {
        "type": "GRID", "id": "GRID_ID",
        "children": ["ROW-1", "ROW-2", "ROW-3"],
        "parents": ["ROOT_ID"],
    },
    # Row 1: two big numbers
    "ROW-1": {
        "type": "ROW", "id": "ROW-1",
        "children": ["CHART-1", "CHART-2"],
        "parents": ["ROOT_ID", "GRID_ID"],
        "meta": {"background": "BACKGROUND_TRANSPARENT"},
    },
    "CHART-1": {
        "type": "CHART", "id": "CHART-1",
        "children": [],
        "parents": ["ROOT_ID", "GRID_ID", "ROW-1"],
        "meta": {"chartId": chart_ids[0], "width": 6, "height": 100},
    },
    "CHART-2": {
        "type": "CHART", "id": "CHART-2",
        "children": [],
        "parents": ["ROOT_ID", "GRID_ID", "ROW-1"],
        "meta": {"chartId": chart_ids[1], "width": 6, "height": 100},
    },
    # Row 2: pie + capture rate line
    "ROW-2": {
        "type": "ROW", "id": "ROW-2",
        "children": ["CHART-3", "CHART-4"],
        "parents": ["ROOT_ID", "GRID_ID"],
        "meta": {"background": "BACKGROUND_TRANSPARENT"},
    },
    "CHART-3": {
        "type": "CHART", "id": "CHART-3",
        "children": [],
        "parents": ["ROOT_ID", "GRID_ID", "ROW-2"],
        "meta": {"chartId": chart_ids[2], "width": 4, "height": 350},
    },
    "CHART-4": {
        "type": "CHART", "id": "CHART-4",
        "children": [],
        "parents": ["ROOT_ID", "GRID_ID", "ROW-2"],
        "meta": {"chartId": chart_ids[3], "width": 8, "height": 350},
    },
    # Row 3: OCR length line + top window titles table
    "ROW-3": {
        "type": "ROW", "id": "ROW-3",
        "children": ["CHART-5", "CHART-6"],
        "parents": ["ROOT_ID", "GRID_ID"],
        "meta": {"background": "BACKGROUND_TRANSPARENT"},
    },
    "CHART-5": {
        "type": "CHART", "id": "CHART-5",
        "children": [],
        "parents": ["ROOT_ID", "GRID_ID", "ROW-3"],
        "meta": {"chartId": chart_ids[4], "width": 8, "height": 350},
    },
    "CHART-6": {
        "type": "CHART", "id": "CHART-6",
        "children": [],
        "parents": ["ROOT_ID", "GRID_ID", "ROW-3"],
        "meta": {"chartId": chart_ids[5], "width": 4, "height": 350},
    },
}

# Create dashboard
dash_body = {
    "dashboard_title": "ScreenPipe Live",
    "slug": "screenpipe-live",
    "position_json": json.dumps(components),
    "json_metadata": json.dumps({"refresh_frequency": 10}),
    "published": True,
}
r = s.post(f"{BASE}/api/v1/dashboard/", json=dash_body)
dash = r.json()
dash_id = dash.get("id")
if not dash_id:
    print(f"ERROR: {dash}", file=sys.stderr)
    sys.exit(1)
print(f"  dashboard {dash_id}: ScreenPipe Live")

# Link charts to dashboard
for cid in chart_ids:
    s.put(f"{BASE}/api/v1/chart/{cid}", json={"dashboards": [dash_id]})

print(f"\nDone! {BASE}/superset/dashboard/{dash_id}/")
