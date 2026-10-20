/**
 * GPU part of the benchmark: renders a heavy fragment shader (a small ray-marched scene) at
 * 1920×1080 in an offscreen WebGL2 canvas for a few seconds and returns frames per second.
 * The webview draws with the real GPU, so the score follows GPU speed and driver changes.
 * Returns null when WebGL2 isn't available.
 */
const VERT = `#version 300 es
in vec2 p; void main() { gl_Position = vec4(p, 0.0, 1.0); }`;

const FRAG = `#version 300 es
precision highp float;
uniform vec2 res; uniform float t; out vec4 o;
float map(vec3 p) {
  vec3 q = mod(p + 2.0, 4.0) - 2.0;
  float s = length(q) - 0.9 - 0.15 * sin(t + p.x * 1.7 + p.z * 1.3);
  float b = length(max(abs(q) - vec3(0.6), 0.0)) - 0.15;
  return min(s, max(b, -s + 0.2));
}
vec3 normal(vec3 p) {
  vec2 e = vec2(0.001, 0.0);
  return normalize(vec3(map(p + e.xyy) - map(p - e.xyy), map(p + e.yxy) - map(p - e.yxy), map(p + e.yyx) - map(p - e.yyx)));
}
void main() {
  vec2 uv = (gl_FragCoord.xy * 2.0 - res) / res.y;
  vec3 ro = vec3(0.0, 0.0, t * 1.5), rd = normalize(vec3(uv, 1.4));
  float d = 0.0; vec3 col = vec3(0.0);
  for (int i = 0; i < 96; i++) {
    vec3 p = ro + rd * d; float h = map(p);
    if (h < 0.001) { vec3 n = normal(p); col = 0.5 + 0.5 * n; col *= max(dot(n, normalize(vec3(1.0, 1.0, -1.0))), 0.2); break; }
    d += h; if (d > 40.0) break;
  }
  o = vec4(col * exp(-0.04 * d), 1.0);
}`;

export async function gpuBench(seconds = 6): Promise<number | null> {
  const canvas = new OffscreenCanvas(1920, 1080);
  const gl = canvas.getContext('webgl2', { antialias: false, powerPreference: 'high-performance' }) as WebGL2RenderingContext | null;
  if (!gl) return null;
  const sh = (type: number, src: string) => {
    const s = gl.createShader(type)!;
    gl.shaderSource(s, src);
    gl.compileShader(s);
    return s;
  };
  const prog = gl.createProgram()!;
  gl.attachShader(prog, sh(gl.VERTEX_SHADER, VERT));
  gl.attachShader(prog, sh(gl.FRAGMENT_SHADER, FRAG));
  gl.linkProgram(prog);
  if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) return null;
  gl.useProgram(prog);
  const buf = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, buf);
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), gl.STATIC_DRAW);
  const loc = gl.getAttribLocation(prog, 'p');
  gl.enableVertexAttribArray(loc);
  gl.vertexAttribPointer(loc, 2, gl.FLOAT, false, 0, 0);
  gl.viewport(0, 0, 1920, 1080);
  gl.uniform2f(gl.getUniformLocation(prog, 'res'), 1920, 1080);
  const uT = gl.getUniformLocation(prog, 't');
  const px = new Uint8Array(4);
  // Reading one pixel back waits for the frame to finish, so frames are really counted
  const frame = (i: number) => { gl.uniform1f(uT, i * 0.016); gl.drawArrays(gl.TRIANGLES, 0, 3); gl.readPixels(0, 0, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, px); };
  for (let i = 0; i < 5; i++) frame(i);
  const start = performance.now();
  let frames = 0;
  while (performance.now() - start < seconds * 1000) {
    frame(frames++);
    // Let the page breathe every few frames so the progress UI stays alive
    if (frames % 4 === 0) await new Promise(r => setTimeout(r, 0));
  }
  const fps = frames / ((performance.now() - start) / 1000);
  gl.getExtension('WEBGL_lose_context')?.loseContext();
  return fps;
}
