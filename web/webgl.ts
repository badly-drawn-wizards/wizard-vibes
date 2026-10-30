// The WebGL2 fallback, for browsers without WebGPU (or with ?webgl in the
// URL): instead of Futhark's compute kernels, a fragment shader generated
// from the same program (dev/webgl.ts writes it into each build as
// wizard.gl.json), drawn over a full-screen triangle. Its uniform block
// holds the drawer's uniforms, the drive record and the camera, each a
// 4-byte scalar at 4 * its index (std140 for scalars). There are no spells
// or collisions here: those need the compute side.

import type { Drive } from "./flight.ts";

type Value = number | boolean | number[];

interface GlField {
  name: string;
  type: "f32" | "i32";
  from: { kind: "param"; id: string; index?: number } | { kind: "drive"; id: string } | { kind: "arg"; id: string };
}

interface GlShader {
  glsl: string;
  block: string;
  fields: GlField[];
}

export interface GlArgs {
  vw: number;
  h: number;
  tm: number;
  yaw: number;
  pitch: number;
  dst: number;
}

const VERTEX = `#version 300 es
void main() {
  // a triangle covering the viewport
  vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
  gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}`;

export class GlRenderer {
  readonly gl: WebGL2RenderingContext;
  version: string | null = null;
  private program: WebGLProgram | null = null;
  private fields: GlField[] = [];
  private ubo: WebGLBuffer;
  private data = new ArrayBuffer(0);

  constructor(canvas: HTMLCanvasElement) {
    const gl = canvas.getContext("webgl2", { antialias: false, depth: false, preserveDrawingBuffer: false });
    if (!gl) throw new Error("Neither WebGPU nor WebGL2 is available in this browser.");
    this.gl = gl;
    this.ubo = gl.createBuffer()!;
  }

  // Loads a build's fragment shader; resolves once it is compiled and linked
  // (in the background where the browser can: it is large).
  async load(version: string): Promise<void> {
    const res = await fetch(`fut/${version}/wizard.gl.json`);
    if (!res.ok) throw new Error(`build ${version} has no WebGL shader (wizard.gl.json: ${res.status})`);
    const shader = (await res.json()) as GlShader;
    const gl = this.gl;
    const compile = (type: number, src: string) => {
      const s = gl.createShader(type)!;
      gl.shaderSource(s, src);
      gl.compileShader(s);
      return s;
    };
    const vs = compile(gl.VERTEX_SHADER, VERTEX);
    const fs = compile(gl.FRAGMENT_SHADER, shader.glsl);
    const program = gl.createProgram()!;
    gl.attachShader(program, vs);
    gl.attachShader(program, fs);
    gl.linkProgram(program);
    const parallel = gl.getExtension("KHR_parallel_shader_compile");
    if (parallel) {
      while (!gl.getProgramParameter(program, parallel.COMPLETION_STATUS_KHR)) await new Promise((r) => setTimeout(r, 50));
    }
    if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
      const log = gl.getShaderInfoLog(fs) || gl.getShaderInfoLog(vs) || gl.getProgramInfoLog(program);
      throw new Error(`WebGL shader failed to compile:\n${(log ?? "").slice(0, 2000)}`);
    }
    const index = gl.getUniformBlockIndex(program, shader.block);
    gl.uniformBlockBinding(program, index, 0);
    if (this.program) gl.deleteProgram(this.program);
    this.program = program;
    this.fields = shader.fields;
    this.data = new ArrayBuffer(Math.ceil((shader.fields.length * 4) / 16) * 16);
    this.version = version;
  }

  get ready(): boolean {
    return this.program !== null;
  }

  // Draws a frame of vw x h into the canvas.
  draw(values: Record<string, Value>, d: Drive, a: GlArgs): void {
    const gl = this.gl;
    if (!this.program) return;
    const f = new Float32Array(this.data);
    const i = new Int32Array(this.data);
    this.fields.forEach((field, k) => {
      const src = field.from;
      let v: Value;
      if (src.kind === "param") {
        const p = values[src.id];
        v = src.index !== undefined ? (p as number[])[src.index] : p;
      } else if (src.kind === "drive") v = (d as unknown as Record<string, number | boolean>)[src.id];
      else v = a[src.id as keyof GlArgs];
      const n = typeof v === "boolean" ? Number(v) : (v as number);
      if (field.type === "f32") f[k] = n;
      else i[k] = Math.round(n);
    });
    gl.bindBuffer(gl.UNIFORM_BUFFER, this.ubo);
    gl.bufferData(gl.UNIFORM_BUFFER, this.data, gl.DYNAMIC_DRAW);
    gl.bindBufferBase(gl.UNIFORM_BUFFER, 0, this.ubo);
    gl.viewport(0, 0, a.vw, a.h);
    gl.useProgram(this.program);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
  }
}
