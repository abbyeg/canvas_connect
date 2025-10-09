// Constants
const TILE = 800;
const TILE_WIDTH = 800;
const TILE_HEIGHT = 800;
const WS_URL = `ws://${location.host}/ws`;
const BASE = document.getElementById('base');
const OVERLAY = document.getElementById('overlay');
const BCTX = BASE.getContext('2d'); // CanvasRenderingContext2D
const OCTX = OVERLAY.getContext('2d'); // CanvasRenderingContext2D
const DPR = devicePixelRatio || 1; 
const TILES = new Map(); // "tx,ty" -> {version, bmp}
const DIRTY = new Set(); // keys needing redraw
const CANVAS_OPERATIONS = new Map([
    [0, "source-over"], // default drawing
    [1, "destination-out"], // eraser
]);
const STAGE_WRAP = document.querySelector('.stage-wrap');
const COORD = document.getElementById('coord');
const WORKSPACE = document.getElementById('workspace');
const COLORS = ['#1e1b16','#ff4da3','#39d6e1','#ffd44a','#9bd96b','#8a5be0','#ff7b3a','#6e6243','#cccccc'];
const PALETTE = document.getElementById('palette');
const BRUSH_LABEL = document.getElementById('brushLabel');

// Variables
let view = { x: 0, y: 0, scale: 1, tool: 0, color: '#222222', size: 6 };
let down = false;
let q = [];
let ws;
let draw_radius = 6;
const query_string = window.location.search;
const urlParams = new URLSearchParams(query_string);
const room_id = urlParams.get('r') || '';
// const room_id = "a82b17b7-0c98-4aae-afea-8a481af601b3";
// Brush Params
const BASE_RADIUS = 5; // px at pressure == 1
const SPACING_PCT = 0.1; // 25% of diameter
const HARDNESS = 0.6;
const FLOW = 0.2; // per-dab alpha
const OPACITY = 1.0;
const PRESSURE_G = 0.5; // pressure curve gamma
const BRUSH_CACHE = new Map(); // key `${r}|${HARDNESS}` -> OffScreenCanvas

function clamp(z, a, b) {
    return Math.max(a, Math.min(b, z));
}

function getPos(evt) {
    const rect = WORKSPACE.getBoundingClientRect();
    const x = (evt.clientX - rect.left)/view.scale;
    const y = (evt.clientY - rect.top)/view.scale;
    return {x: clamp(x, 0, TILE_WIDTH), y: clamp(y, 0, TILE_HEIGHT)};
}

function onPointerMove(e) {
    const p = getPos(e);
    COORD.textContent = `x:${Math.round(p.x)} y:${Math.round(p.y)}`;
}

OVERLAY.addEventListener('mousemove', onPointerMove);

function updateBrushLabel() {
    const txt = `${view.size}px · ${view.color}`;
    BRUSH_LABEL.textContent = txt;
}

COLORS.forEach(hex => {
    const sw = document.createElement('button');
    sw.className='color-swatch';
    sw.style.background = hex;
    sw.title = hex;
    sw.setAttribute('aria-label', 'color '+hex);
    sw.onclick = () => {
        view.color = hex;
        color.value = hex;
        updateBrushLabel();
    };
    PALETTE?.appendChild(sw); 
});


async function resize() {
  const newDPR = window.devicePixelRatio || 1;
  if (newDPR === DPR) return;          // no backing-store reset

  DPR = newDPR;
  await resizeBackingStore();
}

async function resizeBackingStore() {
    const snap = await createImageBitmap(BASE); // preserve pixels

    const cssW = 800, cssH = 800;
    for (const c of [BASE, OVERLAY]) {
        c.width  = Math.floor(cssW * DPR);
        c.height = Math.floor(cssH * DPR);
        c.style.width  = cssW + 'px';
        c.style.height = cssH + 'px';
    }
    BCTX.setTransform(DPR,0,0,DPR,0,0);
    BCTX.drawImage(snap, 0, 0, cssW, cssH); // restore
    OCTX.setTransform(DPR,0,0,DPR,0,0);
    OCTX.clearRect(0,0,OVERLAY.width,OVERLAY.height); // overlay is ephemeral
}

addEventListener('resize', resize);
resizeBackingStore();

function copyToClipboard(id) {
    const copyText = document.getElementById(id);
    copyText.select();
    copyText.setSelectionRange(0, 99999); // For mobile devices

    navigator.clipboard.writeText(copyText.value)
        .then(() => {
            document.getElementById("message").innerText = "Text copied to clipboard!";
            setTimeout(() => {
                document.getElementById("message").innerText = "";
            }, 2000); // Clear message after 2 seconds
        })
        .catch(err => {
            console.error('Failed to copy text: ', err);
            document.getElementById("message").innerText = "Failed to copy text.";
        });
}

function key(tx, ty) {
    return `${tx},${ty}`;
}

function decodeKey(k) {
    const [x,y] = k.split(',').map(Number);
    return [x,y];
}

function worldToScreen(x, y) {
    return [(x-view.x) * view.scale, (y-view.y) * view.scale];
}

function screenToWorld(x, y) {
    return [x/view.scale + view.x, y/view.scale + view.y];
}

function render() {
    BCTX.setTransform(1,0,0,1,0,0);
    BCTX.clearRect(0, 0, BASE.width, BASE.height);
    BCTX.setTransform(DPR,0,0,DPR,0,0);

    for (const k of TILES.keys()) {
        const { bmp } = TILES.get(k);
        if (!bmp) continue;
        const [tx, ty] = decodeKey(k);
        const [sx, sy] = worldToScreen(tx * TILE, ty * TILE); // TILE = 800
        const s = TILE * view.scale;
        BCTX.drawImage(bmp, sx, sy, s, s);
    }
    DIRTY.clear();
}

function setMode(drawMode) {
    view.tool = drawMode;
    BCTX.globalCompositeOperation = CANVAS_OPERATIONS.get(view.tool) ?? "source-over"; 
    OCTX.globalCompositeOperation = CANVAS_OPERATIONS.get(view.tool) ?? "source-over";  
}

/* -****- Painting -****- */

OVERLAY.addEventListener('pointerdown', e => {
    OVERLAY.setPointerCapture(e.pointerId);
    down = true;
    push(e);
});

OVERLAY.addEventListener('pointermove', e => {
    if (down) {
        push(e);
    }
});

OVERLAY.addEventListener('pointerup', e => {
    down = false;
    flush();
    prev = null; carry = 0;
});

let prev = null;
let prev2 = null;
let carry = 0;

function push(e) {
    const rect = OVERLAY.getBoundingClientRect();
    const evs = e.getCoalescedEvents ? e.getCoalescedEvents() : [e];
    for (const ev of evs) {
        const sx = (ev.clientX - rect.left);
        const sy = (ev.clientY - rect.top);
        const [x, y] = screenToWorld(sx, sy);
        const pressure = Math.max(0.01, ev.pressure ?? 1);
        emitDabs(x, y, pressure);
    }
}

function emitDabs(x, y, pressure) {
    const r = BASE_RADIUS * Math.pow(pressure, PRESSURE_G);
    const stepBase = Math.max(0.5, SPACING_PCT * (2*r)); // clamp tiny steps
    if (!prev) {
        q.push(x, y, r, pressure);
        prev = {x, y, r};
        prev2 = null;
        return;
    }
    let dx = x - prev.x;
    let dy = y - prev.y;
    let dist = Math.hypot(dx, dy);
    if (dist === 0) return;

    let curvature = 0;
    if (prev2) {
        const v1x = prev.x - prev2.x;
        const v1y = prev.y - prev2.y;
        const v2x = x - prev.x;
        const v2y = y - prev.y;
        const dot = (v1x*v2x + v1y*v2y) / ((Math.hypot(v1x,v1y)*Math.hypot(v2x,v2y))+1e-6);
        const ang = Math.acos(Math.max(-1, Math.min(1, dot)));
        curvature = ang;
    }
    
    const step = stepBase * (1 - 0.6 * Math.min(1, curvature / 1.0));
    let ux = dx / dist;
    let uy = dy / dist;
    let t = carry;
    while (t <= dist) {
        const px = prev.x + ux * t;
        const py = prev.y + uy * t;
        q.push(px, py, r, 1);
        t += step;
    }
    carry = t - dist;
    prev2 = prev;
    prev = {x, y, r};
}

function flush() {
    if (!q.length || ws.readyState !== 1) {
        q.length = 0;
        return;
    }
    const msg = { type: "dabs", tool: view.tool, dabs: q };
    ws.send(JSON.stringify(msg));
    drawDabsLocal(OCTX, q, view.tool); // local echo
    q = [];
}
// 12 ms batching
setInterval(() => {if (down) flush(); }, 12);

// local echo
function drawDabsLocal(ctx, buf, tool) {
    ctx.globalCompositeOperation = CANVAS_OPERATIONS.get(tool) ?? "source-over";

    for (let i = 0; i < buf.length; i += 4) {
        const [wx,wy,r,pressure] = buf.slice(i, i+4);
        const [sx, sy] = worldToScreen(wx, wy);
        const rr = Math.max(1, Math.round(r));
        const tip = getBrush(rr, HARDNESS);
        ctx.globalAlpha = Math.min(1, FLOW * pressure); // flow per-dab
        const s = (rr*2)*view.scale;
        ctx.arc(sx, sy, r*view.scale, 0, Math.PI*2);
        ctx.drawImage(tip, sx - s/2, sy - s/2, s, s);
    }

    ctx.globalAlpha = 1;
    ctx.globalCompositeOperation = CANVAS_OPERATIONS.get(tool);
}

function getBrush(r, hardness) {
    const key = `${r}|${hardness}`;
    let tip = BRUSH_CACHE.get(key);
    if (tip) return tip;

    const d = r*2;
    const oc = (typeof OffscreenCanvas !== 'undefined') ? new OffscreenCanvas(d, d) : document.createElement('canvas');
    oc.width = d;
    oc.height = d;
    const ctx = oc.getContext('2d');
    
    const g = ctx.createRadialGradient(r, r, 0, r, r, r);
    const h = Math.max(0, Math.min(1, hardness));
    g.addColorStop(0, 'rgba(0,0,0,1)');
    g.addColorStop(h, 'rgba(0,0,0,1)');
    g.addColorStop(1, 'rgba(0,0,0,0)');
    ctx.fillStyle = g;
    ctx.fillRect(0, 0, d, d);
    BRUSH_CACHE.set(key, oc);
    return oc;
}

function b64ToU8(b64) {
  const bin = atob(b64);
  const u8 = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) u8[i] = bin.charCodeAt(i);
  return u8;
}

/* -****- WebSocket -****- */
function connect(){
    if (room_id === "") {
       ws = new WebSocket(WS_URL + room_id); 
    } else {
        ws = new WebSocket(`${WS_URL}/${room_id}`);
    }
    ws.onopen = () => {
        const known = {};
        for (const [k,v] of TILES) known[k] = v.version || 0;
        ws.send(JSON.stringify({type: "join", known }));
    };

    ws.onmessage = async (e) => {
        const msg = JSON.parse(e.data);
        if (msg.type === "tile_patch") {
            console.log("PATCH RECEIVED");

            const k = key(msg.tx, msg.ty);
            const u8 = b64ToU8(msg.png_base64);
            const blob = new Blob([u8], { type: "image/png" });
            
            let bmp;
            try {
                bmp = await createImageBitmap(blob);
            } catch {
                const img = new Image();
                img.src = URL.createObjectURL(blob);
                await img.decode();
                bmp = img;
            }

            const cur = TILES.get(k);
            if (!cur || msg.version > cur.version) {
                console.log(`replacing tile with patch`);
                TILES.set(k, { version: msg.version, bmp });
                DIRTY.add(k);
                requestAnimationFrame(render);
            }
            return;
        }
        if (msg.type === "dabs") { 
            drawDabsLocal(OCTX, msg.dabs, msg.tool);
        }
        if (msg.type === "debug") {
            document.getElementById("connectionPort").textContent = String(msg.port);
            document.getElementById("roomChip").textContent = 'ROOM: ' + (msg.room_id ? msg.room_id : '—');
            return;
        }
    };
    ws.onerror = async (e) => {
      console.error(`WebSocket error: ${JSON.stringify(e)}`);
      // TODO: Implement error handling logic here, display error to user or attempt reconnection
    };

    ws.onclose = () => setTimeout(connect, 1000);
}

connect();
