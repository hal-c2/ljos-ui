(function () {
  // The script half of a Canvas's 2D context: properties and methods that
  // call the native half (context.rs) through read, write and op.
  if (globalThis.__ljosCanvasContext) return;
  const PROPS = ["lineWidth", "lineCap", "lineJoin", "miterLimit", "lineDashOffset",
    "globalAlpha", "globalCompositeOperation", "font", "textAlign", "textBaseline",
    "shadowBlur", "shadowColor", "shadowOffsetX", "shadowOffsetY", "fillRule"];
  const OPS = ["arc", "arcTo", "beginPath", "bezierCurveTo", "clearRect", "clip", "closePath",
    "ellipse", "fill", "fillRect", "fillText", "lineTo", "moveTo", "quadraticCurveTo", "rect",
    "resetTransform", "rotate", "roundedRect", "scale", "setTransform", "shear", "stroke",
    "strokeRect", "strokeText", "text", "transform", "translate"];
  const PAINTS = new Set(["fill", "stroke", "fillRect", "strokeRect", "fillText", "strokeText"]);

  function CanvasGradient(kind, args) {
    this.__kind = kind;
    this.__args = args.map(Number);
    this.__stops = [];
    this.__version = 0;
  }
  CanvasGradient.prototype.addColorStop = function (offset, color) {
    offset = Number(offset);
    if (!(offset >= 0 && offset <= 1)) return;
    this.__stops.push([offset, color]);
    this.__version++;
  };
  function CanvasPattern(desc) { this.__desc = desc; this.__version = 0; }

  // An item has a width, a height and a data list too: image data is told
  // by its pixel array.
  function isImageData(v) {
    return v !== null && typeof v === "object" && v.data instanceof Uint8ClampedArray;
  }
  function dataOf(d) { return ["data", d.width, d.height, Array.from(d.data)]; }
  function imageOf(v) { return isImageData(v) ? dataOf(v) : v; }
  function styleOf(v) {
    if (v instanceof CanvasGradient) return [v.__kind, v.__args, v.__stops];
    if (v instanceof CanvasPattern) return v.__desc;
    return v;
  }

  globalThis.__ljosCanvasContext = function (n, canvas) {
    const ctx = {};
    // A gradient or pattern set as a style is read back as itself; the
    // version is that of the stops the native half has.
    let objs = { fillStyle: null, strokeStyle: null };
    let sent = { fillStyle: -1, strokeStyle: -1 };
    let saved = [];
    function sync() {
      for (const p of ["fillStyle", "strokeStyle"]) {
        const o = objs[p];
        if (o !== null && sent[p] !== o.__version) {
          n.write(p, styleOf(o));
          sent[p] = o.__version;
        }
      }
    }
    Object.defineProperty(ctx, "canvas", { enumerable: true, get: () => canvas });
    for (const p of PROPS) {
      Object.defineProperty(ctx, p, {
        enumerable: true,
        get: () => n.read(p),
        set: (v) => { n.write(p, v); },
      });
    }
    for (const p of ["fillStyle", "strokeStyle"]) {
      Object.defineProperty(ctx, p, {
        enumerable: true,
        get: () => (objs[p] !== null ? objs[p] : n.read(p)),
        set: (v) => {
          const made = v instanceof CanvasGradient || v instanceof CanvasPattern;
          if (n.write(p, styleOf(v))) {
            objs[p] = made ? v : null;
            sent[p] = made ? v.__version : -1;
          }
        },
      });
    }
    for (const name of OPS) {
      ctx[name] = function () {
        if (PAINTS.has(name)) sync();
        const r = n.op(name, Array.prototype.slice.call(arguments));
        return r === undefined ? ctx : r;
      };
    }
    ctx.save = function () {
      saved.push([objs.fillStyle, objs.strokeStyle, sent.fillStyle, sent.strokeStyle]);
      n.op("save", []);
      return ctx;
    };
    ctx.restore = function () {
      const s = saved.pop();
      if (s !== undefined) {
        objs = { fillStyle: s[0], strokeStyle: s[1] };
        sent = { fillStyle: s[2], strokeStyle: s[3] };
      }
      n.op("restore", []);
      return ctx;
    };
    ctx.reset = function () {
      objs = { fillStyle: null, strokeStyle: null };
      sent = { fillStyle: -1, strokeStyle: -1 };
      saved = [];
      n.op("reset", []);
      return ctx;
    };
    ctx.isPointInPath = function (x, y) { return n.op("isPointInPath", [x, y]); };
    ctx.measureText = function (text) { return { width: n.op("measureText", [String(text)]) }; };
    ctx.setLineDash = function (dashes) {
      n.op("setLineDash", [Array.from(dashes || [], Number)]);
      return ctx;
    };
    ctx.getLineDash = function () { return Array.from(n.op("getLineDash", [])); };
    ctx.createLinearGradient = function (x0, y0, x1, y1) {
      return new CanvasGradient("linear", [x0, y0, x1, y1]);
    };
    ctx.createRadialGradient = function (x0, y0, r0, x1, y1, r1) {
      return new CanvasGradient("radial", [x0, y0, r0, x1, y1, r1]);
    };
    ctx.createConicalGradient = function (x, y, angle) {
      return new CanvasGradient("conical", [x, y, angle]);
    };
    ctx.createPattern = function (image, repetition) {
      // Qt's createPattern(colour, brushStyle).
      if (typeof repetition === "number") return new CanvasPattern(["solid", image, repetition]);
      const r = repetition === undefined || repetition === null || repetition === ""
        ? "repeat" : String(repetition);
      return new CanvasPattern(["pattern", imageOf(image), r]);
    };
    ctx.drawImage = function (image) {
      const args = Array.prototype.slice.call(arguments, 1);
      n.op("drawImage", [imageOf(image)].concat(args));
      return ctx;
    };
    ctx.createImageData = function (a, b) {
      let w, h;
      if (isImageData(a)) { w = a.width; h = a.height; }
      else { w = Math.abs(Math.round(Number(a))); h = Math.abs(Math.round(Number(b))); }
      return { width: w, height: h, data: new Uint8ClampedArray(w * h * 4) };
    };
    ctx.getImageData = function (x, y, w, h) {
      const r = n.op("getImageData", [x, y, w, h]);
      if (r === undefined) return undefined;
      return { width: r[0], height: r[1], data: Uint8ClampedArray.from(r[2]) };
    };
    ctx.putImageData = function (data) {
      if (!isImageData(data)) return ctx;
      const args = Array.prototype.slice.call(arguments, 1);
      n.op("putImageData", [dataOf(data)].concat(args));
      return ctx;
    };
    ctx.drawFocusRing = function () { return ctx; };
    ctx.scrollPathIntoView = function () { return ctx; };
    Object.defineProperty(ctx, "caretBlinkRate", { enumerable: true, value: 0 });
    return ctx;
  };
})
