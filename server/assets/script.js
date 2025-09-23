// Constants
const TILE = 256;
const WS_URL = `ws://${location.host}/ws`;
const BASE = document.getElementById('base');
const OVERLAY = document.getElementById('overlay');
const BCTX = BASE.getContext('2d');
const OCTX = OVERLAY.getContext('2d');
const DPR = devicePixelRatio || 1;
const TILES = new Map(); // "tx,ty" -> {version, bmp}
const DIRTY = new Set(); // keys needing redraw

// Variables
let view = { x: 0, y: 0, scale: 1 };
let down = false;
let q = [];
let tool = 0;
let ws;
OCTX.fillStyle = '#fff';

// handle canvas/screen resize
function resize() {
    const w = Math.floor(window.innerWidth * DPR);
    const h = Math.floor(window.innerHeight * DPR);
    for (const c of [BASE, OVERLAY]) { 
        c.width = w;
        c.height = h;
        c.style.width = (w/DPR)+'px';
        c.style.height = (h/DPR)+'px';
    }
    requestAnimationFrame(render);
}
addEventListener('resize', resize);
resize();

function key(tx, ty) {
    return `${tx},${ty}`;
}

function decodeKey(k) {
    const [x,y] = k.split(',').map(Number);
    return [x,y];
}

function worldToScreen(x, y) {
    return [(x-view.x)*view.scale, (y-view.y)*view.scale];
}

function screenToWorld(x, y) {
    return [x/view.scale+view.x, y/view.scale+view.y];
}

function render() {
    // draw base tiles
    BCTX.setTransform(1,0,0,1,0,0);
    BCTX.clearRect(0,0,BASE.width,BASE.height);
    for (const k of TILES.keys()) {
        const { bmp } = TILES.get(k);
        if (!bmp) continue;
        const [tx, ty] = decodeKey(k);
        const [sx, sy] = worldToScreen(tx*TILE, ty*TILE);
        const s = TILE*view.scale;
        BCTX.drawImage(bmp, sx, sy, s, s);
    }
    // overlay is drawn incrementally by local echo
    DIRTY.clear();
}

/* -****- Painting -****- */

OVERLAY.addEventListener('pointerdown', e => {/* -****- Painting -****- */

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
});

function push(e) {
    const rect = OVERLAY.getBoundingClientRect();
    const sx = (e.clientX - rect.left) * DPR;
    const sy = (e.clientY - rect.top) * DPR;
    const [x, y] = screenToWorld(sx, sy);
    const r = 6; const a = 1.0;
    q.push(x,y,r,a);
    console.log(`pushed data to q: ${x},${y},${r},${a}`);
}

function flush() {
    if (!q.length || ws.readyState !== 1) {
        q.length = 0;
        return;
    }
    const msg = { type: "dabs", tool, dabs: q };
    ws.send(JSON.stringify(msg));
    drawDabsLocal(OCTX, q, tool); // local echo
    q = [];
}
// 12 ms batching
setInterval(() => {if (down) flush(); }, 12);

// local echo
function drawDabsLocal(ctx, buf, t) {
    // console.log(`In drawDabsLocal`);
    ctx.globalCompositeOperation = t === 1 ? "destination-out" : "source-over";
    for (let i = 0; i < buf.length; i += 4) {
        const [wx,wy,r,a] = buf.slice(i, i+4);
        const [sx, sy] = worldToScreen(wx, wy);
        ctx.globalAlpha = a;
        ctx.beginPath();
        ctx.arc(sx,sy,r*view.scale,0, Math.PI*2);
        ctx.fill();
    }
    ctx.globalAlpha=1;
    ctx.globalCompositeOperation = "source-over";
}

/* -****- WebSocket -****- */
function connect(){
    ws = new WebSocket(WS_URL);
    console.log("Websocket connection");
    ws.onopen = () => {
        // send known versions
        const known = {};
        console.log("onopen event");
        console.log(JSON.stringify(ws));
        for (const [k,v] of TILES) known[k] = v.version || 0;
        ws.send(JSON.stringify({type: "join", room_id: "default", known }));
    };

    ws.onmessage = async (e) => {
        const msg = JSON.parse(e.data);
        if (msg.type === "tile_patch") {
            const k = key(msg.tx, msg.ty);
            // decode PNG -> image bitmap
            const blob = await (await fetch("data:image/png;base64,"+msg.png_base64)).blob();
            const bmp = await createImageBitmap(blob);
            const cur = TILES.get(k);
            if (!cur || msg.version > cur.version) {
                TILES.set(k, { version: msg.version, bmp });
                DIRTY.add(k);
                requestAnimationFrame(render);
            }
            return;
        }
        if (msg.type === "dabs") {
            console.log("drawing dabs local");
            drawDabsLocal(OCTX, msg.dabs, msg.tool);
        }
        if (msg.type === "debug") {
            console.log("writing debug message");
            document.getElementById("connection-port").textContent = String(msg.port);
            return;
        }
    };
    ws.onerror = async (e) => {
      console.error(`WebSocket error: ${JSON.stringify(e)}`);
      // Implement error handling logic here, e.g., display a message to the user, attempt reconnection.
    };

    ws.onclose = () => setTimeout(connect, 1000);
}
console.log("websocket connect soon?");
connect();

/* -****- MISC -****- */

// const socket = new WebSocket('ws://localhost:3000/ws');

// socket.addEventListener('open', function (event) {
//     socket.send('Hello Server!');
// });

// socket.addEventListener('message', function (event) {
//     console.log('Message from server ', event.data);
// });


// setTimeout(() => {
//     const obj = { hello: "world" };
//     const blob = new Blob([JSON.stringify(obj, null, 2)], {
//       type: "application/json",
//     });
//     console.log("Sending blob over websocket");
//     socket.send(blob);
// }, 1000);

// setTimeout(() => {
//     socket.send('About done here...');
//     console.log("Sending close over websocket");
//     socket.close(3000, "Crash and Burn!");
// }, 3000);
