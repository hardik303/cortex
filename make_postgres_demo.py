#!/usr/bin/env python3
"""
Cortex Memory Agent — "How did I solve the PostgreSQL scaling issue?" demo video.
~50s, 1920×1080, 30fps, H.264.
"""

import os, math, subprocess, textwrap
from PIL import Image, ImageDraw, ImageFont, ImageFilter

W, H  = 1920, 1080
FPS   = 30
OUT   = "cortex_postgres_demo.mp4"
FDIR  = "pg_frames"
os.makedirs(FDIR, exist_ok=True)

# ── Palette ───────────────────────────────────────────────────────────────────
BG          = ( 8,  9, 14)
PANEL       = (13, 15, 23)
CARD        = (18, 21, 32)
CARD2       = (22, 26, 38)
BORDER      = (38, 44, 60)
BORDER2     = (55, 63, 82)

INDIGO      = (99, 102, 241)
INDIGO_LT   = (129, 140, 248)
VIOLET      = (167, 139, 250)
CYAN        = ( 34, 211, 238)
EMERALD     = ( 52, 211, 153)
AMBER       = (251, 191,  36)
ROSE        = (251, 113, 133)
PINK        = (236, 72, 153)

TEXT1       = (236, 238, 244)
TEXT2       = (160, 168, 185)
TEXT3       = (100, 110, 130)
TEXT4       = ( 60,  68,  85)
WHITE       = (255, 255, 255)
BLACK       = (  0,   0,   0)

# ── Fonts ─────────────────────────────────────────────────────────────────────
def F(size, bold=False):
    for p in [f"/System/Library/Fonts/{'SFPro-Bold' if bold else 'SFPro'}.ttf",
              "/System/Library/Fonts/Supplemental/Arial Bold.ttf" if bold else "/System/Library/Fonts/Supplemental/Arial.ttf",
              "/System/Library/Fonts/Helvetica.ttc"]:
        if os.path.exists(p):
            try: return ImageFont.truetype(p, size)
            except: pass
    return ImageFont.load_default()

def M(size):
    for p in ["/System/Library/Fonts/Menlo.ttc",
              "/System/Library/Fonts/Monaco.ttf",
              "/System/Library/Fonts/Courier.ttc"]:
        if os.path.exists(p):
            try: return ImageFont.truetype(p, size)
            except: pass
    return ImageFont.load_default()

Fb  = lambda s: F(s, bold=True)
Fn  = lambda s: F(s, bold=False)

# ── Layout constants ──────────────────────────────────────────────────────────
SB   = 280          # sidebar width
MAIN = SB + 1       # main content x start
PAD  = 40           # horizontal padding inside main
CX   = MAIN + PAD   # content left edge
CW   = W - CX - PAD # content width

# ── Drawing helpers ───────────────────────────────────────────────────────────
def rr(d, box, r, fill, stroke=None, sw=1):
    """Rounded rect. box=(x0,y0,x1,y1)"""
    x0,y0,x1,y1 = box
    if r <= 0:
        d.rectangle(box, fill=fill, outline=stroke, width=sw)
        return
    d.rectangle([x0+r,y0,x1-r,y1], fill=fill)
    d.rectangle([x0,y0+r,x1,y1-r], fill=fill)
    for cx_,cy_ in [(x0,y0),(x1-2*r,y0),(x0,y1-2*r),(x1-2*r,y1-2*r)]:
        d.ellipse([cx_,cy_,cx_+2*r,cy_+2*r], fill=fill)
    if stroke:
        d.rounded_rectangle(box, radius=r, outline=stroke, width=sw)

def text(d, xy, s, font, fill, anchor="la"):
    d.text(xy, s, font=font, fill=fill, anchor=anchor)

def textw(d, cx, y, s, font, fill):
    d.text((cx,y), s, font=font, fill=fill, anchor="ma")

def tw(d, s, font):
    bb = d.textbbox((0,0), s, font=font)
    return bb[2]-bb[0]

def lerp(a, b, t): return a + (b-a)*t
def ease(t): return t*t*(3-2*t)
def ease_out(t): return 1-(1-t)**3

def blend(fg, bg, a):
    return tuple(int(f*a + b*(1-a)) for f,b in zip(fg,bg))

# ── Sidebar ───────────────────────────────────────────────────────────────────
def draw_sidebar(d, frame_num=0):
    d.rectangle([0,0,SB,H], fill=PANEL)
    d.line([SB,0,SB,H], fill=BORDER, width=1)

    # Logo
    y = 28
    text(d, (22,y), "◈", Fb(22), INDIGO_LT)
    text(d, (50,y), "Cortex", Fb(22), TEXT1)
    text(d, (22,y+26), "Memory Agent", Fn(12), TEXT3)

    # Divider
    d.line([16,74,SB-16,74], fill=BORDER, width=1)

    # KG stats
    y = 88
    text(d, (22,y), "KNOWLEDGE GRAPH", Fn(11), TEXT4)
    y += 20
    stats = [("Frames",   "2,598", INDIGO_LT),
             ("Nodes",    "1,019", EMERALD),
             ("Edges",    "3,247", VIOLET),
             ("Sessions", "12",    CYAN)]
    for label, val, colour in stats:
        rr(d, [16,y,SB-16,y+30], 6, CARD)
        text(d, (26,y+7), label, Fn(13), TEXT2)
        text(d, (SB-22,y+7), val,   Fb(13), colour, anchor="ra")
        y += 36

    # Divider
    d.line([16,y+8,SB-16,y+8], fill=BORDER, width=1)
    y += 22

    # Recent queries
    text(d, (22,y), "RECENT QUERIES", Fn(11), TEXT4)
    y += 20
    queries = [
        ("What scaling issues did I fix?",  True),
        ("Last week's work summary",         False),
        ("Errors in the OCR pipeline",       False),
        ("Commands in the build system",     False),
    ]
    for q, active in queries:
        bg  = blend(INDIGO, CARD, 0.18) if active else CARD
        brd = INDIGO if active else None
        rr(d, [16,y,SB-16,y+36], 6, bg, stroke=brd, sw=1)
        wrapped = q if len(q)<32 else q[:30]+"…"
        text(d, (26,y+10), wrapped, Fn(13), TEXT1 if active else TEXT2)
        y += 42

# ── Top bar ───────────────────────────────────────────────────────────────────
def draw_topbar(d):
    d.rectangle([MAIN,0,W,52], fill=PANEL)
    d.line([MAIN,52,W,52], fill=BORDER, width=1)
    text(d, (CX,15), "Memory Query", Fn(13), TEXT3)
    text(d, (CX+102,15), "›", Fn(13), TEXT4)
    text(d, (CX+116,15), "How did I solve the PostgreSQL scaling issue?", Fn(13), TEXT1)

# ── Full base frame ───────────────────────────────────────────────────────────
def base():
    img = Image.new("RGB",(W,H),BG)
    d   = ImageDraw.Draw(img)
    draw_sidebar(d)
    return img, d

# ─────────────────────────────────────────────────────────────────────────────
# SCENE 1 — Idle (empty query box centred in content area)
# ─────────────────────────────────────────────────────────────────────────────
def scene_idle(t):
    img,d = base()
    mid   = (MAIN + W)//2

    textw(d, mid, 200, "Ask anything about your past work", Fb(28), TEXT2)
    textw(d, mid, 244, "Cortex remembers every session, decision and fix", Fn(16), TEXT3)

    # Input box
    bx0,by0,bx1,by1 = CX, 300, W-PAD, 372
    rr(d, [bx0,by0,bx1,by1], 12, CARD, stroke=BORDER2, sw=1)
    text(d, (bx0+20,by0+18), "Ask about a past debugging session, decision, or fix…", Fn(17), TEXT4)

    # Example chips
    ex = ["How did I fix the Postgres issue?",
          "What did I work on last week?",
          "Which errors repeat most?"]
    px = CX
    for e in ex:
        w_ = tw(d,e,Fn(13))+24
        rr(d,[px,420,px+w_,448],14,CARD2,stroke=BORDER,sw=1)
        text(d,(px+12,427),e,Fn(13),TEXT2)
        px += w_+12

    return img

# ─────────────────────────────────────────────────────────────────────────────
# SCENE 2 — Typing
# ─────────────────────────────────────────────────────────────────────────────
QUESTION = "How did I solve the issue of scaling postgres last time?"

def scene_typing(t, chars):
    img,d = base()
    mid   = (MAIN + W)//2

    textw(d, mid, 200, "Ask anything about your past work", Fb(28), TEXT2)
    textw(d, mid, 244, "Cortex remembers every session, decision and fix", Fn(16), TEXT3)

    bx0,by0,bx1,by1 = CX, 300, W-PAD, 372
    rr(d, [bx0,by0,bx1,by1], 12, CARD, stroke=INDIGO, sw=2)

    partial = QUESTION[:chars]
    text(d,(bx0+20,by0+18), partial, Fn(17), TEXT1)
    # cursor
    if int(t*5)%2==0:
        cw = tw(d,partial,Fn(17))
        d.rectangle([bx0+20+cw+2,by0+16,bx0+20+cw+3,by0+46],fill=INDIGO_LT)

    # chips (dimmed)
    ex = ["How did I fix the Postgres issue?",
          "What did I work on last week?",
          "Which errors repeat most?"]
    px = CX
    for e in ex:
        w_ = tw(d,e,Fn(13))+24
        rr(d,[px,420,px+w_,448],14,CARD,stroke=BORDER,sw=1)
        text(d,(px+12,427),e,Fn(13),TEXT3)
        px += w_+12

    return img

# ─────────────────────────────────────────────────────────────────────────────
# SCENE 3 — Loading / graph search
# ─────────────────────────────────────────────────────────────────────────────
def scene_loading(t):
    img,d = base()
    draw_topbar(d)

    mid_x = (MAIN+W)//2
    mid_y = H//2 - 20

    # Pulsing ring
    for r_,a_ in [(52,0.08),(40,0.15),(28,0.9)]:
        colour = blend(INDIGO, BG, a_)
        d.ellipse([mid_x-r_,mid_y-r_,mid_x+r_,mid_y+r_],outline=colour,width=2)

    # Spinning arc
    ang = (t*360) % 360
    d.arc([mid_x-28,mid_y-28,mid_x+28,mid_y+28],
          ang, ang+220, fill=INDIGO_LT, width=3)

    # Status
    dots = "." * (int(t*3)%4)
    textw(d, mid_x, mid_y+52, f"Searching memory graph{dots}", Fn(17), TEXT2)
    textw(d, mid_x, mid_y+78, "Tracing debugging session · Extracting decisions", Fn(13), TEXT3)

    # Mini node labels floating around spinner
    labels = [("COMMAND",VIOLET,(-140,-60)),("URL",CYAN,(130,-50)),
              ("DECISION",EMERALD,(-110,70)),("ERROR",ROSE,(120,60)),("FILE",AMBER,(0,-90))]
    for lbl,c,off in labels:
        px,py = mid_x+off[0], mid_y+off[1]
        w_ = tw(d,lbl,Fn(11))+16
        rr(d,[px-w_//2,py-11,px+w_//2,py+11],6,CARD2,stroke=c,sw=1)
        textw(d,px,py-4,lbl,Fn(11),c)
        # faint line to center
        d.line([px,py,mid_x,mid_y],fill=blend(c,BG,0.18),width=1)

    return img

# ─────────────────────────────────────────────────────────────────────────────
# SCENE 4 — Answer (the big one)
# ─────────────────────────────────────────────────────────────────────────────

# ── Source pills (like Perplexity citations) ──────────────────────────────────
SOURCES = [
    ("Session Jan 14", "2h 12m", INDIGO_LT),
    ("Session Jan 15", "35min",  VIOLET),
    ("Stack Overflow", "visited",CYAN),
]

# ── Answer content (structured) ───────────────────────────────────────────────
# Each item: (kind, payload)
ANSWER = [
    ("summary_box",  None),          # the "what you did in short" hero card
    ("sp", 20),
    ("sources", None),               # source pills row
    ("sp", 24),

    ("h2", ("🚨", "The Problem",      ROSE)),
    ("sp", 6),
    ("para",
     "Your service hit FATAL: remaining connection slots are reserved under load. "
     "PostgreSQL's default max_connections = 100 was exhausted by short-lived "
     "concurrent connections from the app's thread pool."),
    ("sp", 20),

    ("h2", ("🔬", "What You Tried",   AMBER)),
    ("sp", 6),

    ("attempt", ("1", "Raise max_connections",
                 "Set max_connections = 500 in postgresql.conf.\n"
                 "Result: RAM jumped to 12 GB (each PG backend ≈ 5–10 MB). Reverted.",
                 False, ROSE)),
    ("sp", 8),
    ("attempt", ("2", "Read replicas",
                 "Checked AWS RDS replica docs. Decided overkill — "
                 "problem was connection count, not query load.",
                 False, AMBER)),
    ("sp", 8),
    ("attempt", ("3", "PgBouncer in session mode",
                 "Installed PgBouncer. Hit prepared statement conflicts:\n"
                 'ERROR: prepared statement "s1" already exists',
                 False, AMBER)),
    ("sp", 8),
    ("attempt", ("✓", "PgBouncer — transaction mode",
                 "Switched pool_mode = transaction. Disabled prepared statements "
                 "in app (prepare_threshold=0). Connections dropped from 98 → 18.",
                 True, EMERALD)),
    ("sp", 20),

    ("h2", ("⚙️", "The Config That Worked", EMERALD)),
    ("sp", 8),
    ("code_block", (
        "; pgbouncer.ini",
        "[databases]\n"
        "mydb = host=127.0.0.1 port=5432 dbname=mydb\n"
        "\n"
        "[pgbouncer]\n"
        "pool_mode        = transaction   ; ← key change\n"
        "max_client_conn  = 1000\n"
        "default_pool_size = 20\n"
        "server_pool_size =  5",
    )),
    ("sp", 20),

    ("h2", ("💻", "Commands You Ran",  CYAN)),
    ("sp", 8),
    ("cmd_list", [
        ("psql -c \"SHOW max_connections\"",              "checked baseline"),
        ("psql -c \"SELECT count(*) FROM pg_stat_activity\"", "monitored connections"),
        ("pgbench -c 100 -j 4 -T 30 mydb",               "load test before/after"),
        ("sudo systemctl restart pgbouncer",               "applied config"),
    ]),
    ("sp", 20),

    ("h2", ("📎", "Docs & Resources Visited", VIOLET)),
    ("sp", 8),
    ("url_list", [
        "postgresql.org/docs/current/runtime-config-connection.html",
        "pgbouncer.org/config.html",
        "stackoverflow.com → 'pgbouncer prepared statements error'",
    ]),
    ("sp", 20),

    ("h2", ("✅", "Why It Worked",  EMERALD)),
    ("sp", 6),
    ("bullet_list", [
        "Transaction mode releases PG connection after each txn (not each session).",
        "PgBouncer holds only 20 real connections regardless of client count.",
        "pg_stat_activity: 98 → 18 active connections under identical load.",
        "No code change needed — only connection string pointed to PgBouncer port.",
    ]),
    ("sp", 24),
]

SUMMARY_TEXT = (
    "Installed PgBouncer in transaction pooling mode.\n"
    "Dropped real PG connections 98 → 18 with zero app code changes."
)

def render_answer(d, scroll_y, reveal=1.0):
    """Render the answer into the content area with optional scroll offset."""
    CLIP_TOP = 60        # below topbar
    CLIP_BOT = H - 16

    y = CLIP_TOP + 24 - int(scroll_y)
    total = len(ANSWER)

    # Gate rendering to revealed fraction
    reveal_idx = int(reveal * total)

    for idx, (kind, payload) in enumerate(ANSWER):
        if idx > reveal_idx: break
        alpha = min(1.0, (reveal_idx - idx + 1) * 0.4)

        def c(colour):
            return blend(colour, BG, alpha)

        if y > CLIP_BOT + 100: break

        def visible(): return y > CLIP_TOP - 120

        if kind == "sp":
            y += payload

        elif kind == "summary_box":
            if visible():
                bh = 88
                rr(d,[CX,y,W-PAD,y+bh],12,blend(INDIGO,BG,0.13))
                d.rounded_rectangle([CX,y,W-PAD,y+bh],radius=12,
                                    outline=blend(INDIGO,BG,0.5*alpha),width=1)
                text(d,(CX+16,y+14),"ANSWER",Fn(11),c(INDIGO_LT))
                text(d,(CX+16,y+32),SUMMARY_TEXT.split('\n')[0],Fb(16),c(TEXT1))
                text(d,(CX+16,y+56),SUMMARY_TEXT.split('\n')[1],Fn(14),c(TEXT2))
            y += 96

        elif kind == "sources":
            if visible():
                text(d,(CX,y),"Sources",Fn(12),c(TEXT3))
                sx = CX + 60
                for stitle,smeta,scol in SOURCES:
                    sw_ = tw(d,f"{stitle} · {smeta}",Fn(12))+20
                    rr(d,[sx,y-2,sx+sw_,y+20],10,blend(scol,BG,0.12))
                    d.rounded_rectangle([sx,y-2,sx+sw_,y+20],radius=10,
                                        outline=blend(scol,BG,0.4*alpha),width=1)
                    text(d,(sx+10,y+1),f"{stitle} · {smeta}",Fn(12),c(scol))
                    sx += sw_+10
            y += 30

        elif kind == "h2":
            icon, label, colour = payload
            if visible():
                text(d,(CX,y),icon,Fb(15),c(colour))
                text(d,(CX+28,y),label,Fb(15),c(colour))
            y += 26

        elif kind == "para":
            if visible():
                lines = textwrap.wrap(payload, 90)
                for li,line in enumerate(lines):
                    if CLIP_TOP < y+li*22 < CLIP_BOT:
                        text(d,(CX+8,y+li*22),line,Fn(15),c(TEXT2))
            y += len(textwrap.wrap(payload,90))*22 + 4

        elif kind == "attempt":
            num, title, body, success, colour = payload
            if visible():
                bh = 28 + len(textwrap.wrap(body,80))*20 + 16
                bg_col = blend(colour, BG, 0.08 if success else 0.05)
                rr(d,[CX,y,W-PAD,y+bh],8,bg_col)
                d.rounded_rectangle([CX,y,W-PAD,y+bh],radius=8,
                                    outline=blend(colour,BG,0.35*alpha),width=1)
                # Number badge
                rr(d,[CX+12,y+10,CX+30,y+28],9,blend(colour,BG,0.25))
                textw(d,CX+21,y+11,str(num),Fb(11),c(colour))
                text(d,(CX+38,y+12),title,Fb(14),c(TEXT1))
                lines = textwrap.wrap(body,80)
                for li,line in enumerate(lines):
                    text(d,(CX+38,y+32+li*20),line,M(13) if '=' in line or 'ERROR' in line else Fn(13),
                         c(ROSE if 'ERROR' in line else TEXT2))
            bh = 28 + len(textwrap.wrap(body,80))*20 + 16
            y += bh + 2

        elif kind == "code_block":
            comment, code = payload
            lines = code.split('\n')
            bh = len(lines)*20 + 24
            if visible():
                rr(d,[CX,y,W-PAD,y+bh+4],8,( 10,12,18))
                d.rounded_rectangle([CX,y,W-PAD,y+bh+4],radius=8,
                                    outline=blend(BORDER2,BG,alpha),width=1)
                text(d,(CX+14,y+6),comment,Fn(11),c(TEXT3))
                for li,line in enumerate(lines):
                    ly = y+20+li*20
                    if CLIP_TOP < ly < CLIP_BOT:
                        if line.startswith(';') or line.startswith('['):
                            text(d,(CX+14,ly),line,M(13),c(TEXT3))
                        elif '=' in line:
                            k,_,v = line.partition('=')
                            text(d,(CX+14,ly),k+'=',M(13),c(CYAN))
                            text(d,(CX+14+tw(d,k+'=',M(13)),ly),v,M(13),c(AMBER))
                        elif not line.strip():
                            pass
                        else:
                            text(d,(CX+14,ly),line,M(13),c(TEXT2))
            y += bh + 8

        elif kind == "cmd_list":
            for cmd,desc in payload:
                if visible():
                    bh = 34
                    rr(d,[CX,y,W-PAD,y+bh],6,(12,14,22))
                    d.rounded_rectangle([CX,y,W-PAD,y+bh],radius=6,outline=blend(BORDER,BG,alpha),width=1)
                    d.rectangle([CX,y,CX+4,y+bh],fill=c(CYAN))
                    text(d,(CX+14,y+8),"$",M(13),c(TEXT3))
                    text(d,(CX+28,y+8),cmd,M(13),c(EMERALD))
                    text(d,(W-PAD-tw(d,desc,Fn(12))-12,y+10),desc,Fn(12),c(TEXT3))
                y += 40

        elif kind == "url_list":
            for url in payload:
                if visible():
                    text(d,(CX+8,y),"→",Fn(13),c(VIOLET))
                    text(d,(CX+24,y),url,Fn(13),c(INDIGO_LT))
                y += 22

        elif kind == "bullet_list":
            for b in payload:
                lines = textwrap.wrap(b,88)
                if visible():
                    text(d,(CX+8,y),"•",Fb(14),c(EMERALD))
                    for li,line in enumerate(lines):
                        if CLIP_TOP < y+li*22 < CLIP_BOT:
                            text(d,(CX+22,y+li*22),line,Fn(14),c(TEXT2))
                y += len(lines)*22+4

    return y  # returns last y for scrollbar calc

def scene_answer(scroll_y, reveal=1.0):
    img,d = base()
    draw_topbar(d)
    render_answer(d, scroll_y, reveal)

    # Scrollbar
    content_h = 1400
    vis_h     = H - 76
    sb_h      = max(40,int(vis_h*vis_h/content_h))
    sb_y      = 60 + int((vis_h-sb_h)*scroll_y/max(content_h-vis_h,1))
    rr(d,[W-6,sb_y,W-2,sb_y+sb_h],2,BORDER2)
    return img

# ─────────────────────────────────────────────────────────────────────────────
# Timeline
# ─────────────────────────────────────────────────────────────────────────────
TIMELINE = [
    ("idle",        2.5),   # empty query box
    ("typing",      3.5),   # type the question
    ("loading",     2.0),   # searching memory graph
    ("reveal",      9.0),   # answer reveals section by section
    ("scroll_dn",   9.0),   # scroll down through full report
    ("pause_bot",   2.5),   # hold at bottom
    ("scroll_up",   3.5),   # scroll back to top
    ("hold_top",    2.0),   # final hold
]

frame_n = 0

def save(img):
    global frame_n
    img.save(f"{FDIR}/f{frame_n:05d}.png")
    frame_n += 1

print("Generating frames…")
total_t = 0.0
for phase, dur in TIMELINE:
    n = max(1, int(dur*FPS))
    print(f"  {phase:15s} {dur:.1f}s  ({n} frames)")
    for fi in range(n):
        t = fi/max(n-1,1)
        gt = total_t + fi/FPS

        if phase == "idle":
            save(scene_idle(t))

        elif phase == "typing":
            chars = int(ease(t)*len(QUESTION))
            save(scene_typing(t, chars))

        elif phase == "loading":
            save(scene_loading(gt))

        elif phase == "reveal":
            save(scene_answer(0, reveal=ease_out(t)))

        elif phase == "scroll_dn":
            s = int(ease(t)*820)
            save(scene_answer(s, reveal=1.0))

        elif phase == "pause_bot":
            save(scene_answer(820, reveal=1.0))

        elif phase == "scroll_up":
            s = int((1-ease(t))*820)
            save(scene_answer(s, reveal=1.0))

        elif phase == "hold_top":
            save(scene_answer(0, reveal=1.0))

    total_t += dur

print(f"Generated {frame_n} frames.")

print("Encoding…")
cmd = ["ffmpeg","-y",
       "-framerate",str(FPS),
       "-i",f"{FDIR}/f%05d.png",
       "-c:v","libx264","-preset","slow","-crf","16",
       "-pix_fmt","yuv420p",
       OUT]
r = subprocess.run(cmd, capture_output=True, text=True)
if r.returncode == 0:
    sz = os.path.getsize(OUT)//1024
    print(f"\n✅  {OUT}  ({sz} KB,  {frame_n//FPS}s)")
else:
    print("ffmpeg error:", r.stderr[-800:])
