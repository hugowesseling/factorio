import { orthoMatrix, type Camera } from "./camera";
import {
  beltItemMesh,
  entityMesh,
  FLOATS_PER_VERTEX,
  gridMesh,
  highlightMesh,
  quadsToVertices,
  terrainMesh,
  type MeshQuad,
} from "./mesh";
import type { WorldStore } from "../state/world";
import type { TilePos } from "../sim/terrain";

const VERTEX_SHADER = `#version 300 es
precision highp float;
layout(location = 0) in vec2 a_position;
layout(location = 1) in vec4 a_color;
uniform mat4 u_viewProjection;
out vec4 v_color;
void main() {
  v_color = a_color;
  gl_Position = u_viewProjection * vec4(a_position, 0.0, 1.0);
}`;

const FRAGMENT_SHADER = `#version 300 es
precision highp float;
in vec4 v_color;
out vec4 out_color;
void main() {
  out_color = vec4(v_color.rgb, v_color.a);
}`;

export interface RenderStats {
  drawCalls: number;
  vertices: number;
  quads: number;
}

export interface FrameInput {
  readonly world: WorldStore;
  readonly camera: Camera;
  readonly hover: TilePos | null;
  readonly selected: TilePos | null;
  readonly showGrid: boolean;
  readonly localPlayer: number;
}

export class Renderer {
  readonly stats: RenderStats = { drawCalls: 0, vertices: 0, quads: 0 };

  private readonly gl: WebGL2RenderingContext;
  private readonly program: WebGLProgram;
  private readonly buffer: WebGLBuffer;
  private readonly viewProjection: WebGLUniformLocation | null;
  private readonly attributePosition: number;
  private readonly attributeColor: number;
  private readonly stride: number;
  private capacity = 0;
  private lost = false;

  constructor(private readonly canvas: HTMLCanvasElement) {
    const gl = canvas.getContext("webgl2", {
      alpha: false,
      antialias: false,
      depth: false,
      powerPreference: "high-performance",
    });
    if (gl === null) {
      throw new Error("WebGL2 is not available in this browser");
    }
    this.gl = gl;
    this.program = createProgram(gl, VERTEX_SHADER, FRAGMENT_SHADER);
    this.buffer = requireBuffer(gl.createBuffer());
    this.viewProjection = gl.getUniformLocation(this.program, "u_viewProjection");
    this.attributePosition = gl.getAttribLocation(this.program, "a_position");
    this.attributeColor = gl.getAttribLocation(this.program, "a_color");
    this.stride = FLOATS_PER_VERTEX * Float32Array.BYTES_PER_ELEMENT;

    gl.useProgram(this.program);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.buffer);
    gl.enableVertexAttribArray(this.attributePosition);
    gl.vertexAttribPointer(this.attributePosition, 2, gl.FLOAT, false, this.stride, 0);
    gl.enableVertexAttribArray(this.attributeColor);
    gl.vertexAttribPointer(
      this.attributeColor,
      4,
      gl.FLOAT,
      false,
      this.stride,
      2 * Float32Array.BYTES_PER_ELEMENT,
    );
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
    gl.disable(gl.DEPTH_TEST);

    canvas.addEventListener("webglcontextlost", (event) => {
      event.preventDefault();
      this.lost = true;
    });
  }

  get contextLost(): boolean {
    return this.lost;
  }

  resize(camera: Camera): void {
    const ratio = Math.min(2, Math.max(1, globalThis.devicePixelRatio ?? 1));
    const width = Math.max(1, Math.round(this.canvas.clientWidth * ratio));
    const height = Math.max(1, Math.round(this.canvas.clientHeight * ratio));
    if (this.canvas.width === width && this.canvas.height === height) {
      return;
    }
    this.canvas.width = width;
    this.canvas.height = height;
    camera.viewportWidth = this.canvas.clientWidth;
    camera.viewportHeight = this.canvas.clientHeight;
  }

  render(frame: FrameInput, bounds: { minX: number; minY: number; maxX: number; maxY: number }): RenderStats {
    const gl = this.gl;
    const { camera, world } = frame;

    const layers: MeshQuad[][] = [terrainMesh(world, bounds)];
    if (frame.showGrid) {
      layers.push(gridMesh(bounds));
    }
    layers.push(entityMesh(world));
    layers.push(beltItemMesh(world.allBeltItems));
    layers.push(highlightMesh(frame.hover, frame.selected));

    const drawPlayer = playerQuad(world, frame.localPlayer);
    if (drawPlayer !== null) {
      layers.push([drawPlayer]);
    }

    const quads = layers.flat();
    const vertices = quadsToVertices(quads);

    gl.viewport(0, 0, this.canvas.width, this.canvas.height);
    gl.clearColor(0.06, 0.07, 0.08, 1);
    gl.clear(gl.COLOR_BUFFER_BIT);

    if (vertices.length > 0) {
      this.upload(vertices);
      gl.useProgram(this.program);
      gl.uniformMatrix4fv(this.viewProjection, false, orthoMatrix(camera));
      gl.drawArrays(gl.TRIANGLES, 0, quads.length * 6);
    }

    this.stats.drawCalls = vertices.length > 0 ? 1 : 0;
    this.stats.vertices = vertices.length / FLOATS_PER_VERTEX;
    this.stats.quads = quads.length;
    return this.stats;
  }

  private upload(data: Float32Array): void {
    const gl = this.gl;
    gl.bindBuffer(gl.ARRAY_BUFFER, this.buffer);
    if (data.length > this.capacity) {
      this.capacity = Math.max(data.length * 2, 4096);
      gl.bufferData(gl.ARRAY_BUFFER, this.capacity * Float32Array.BYTES_PER_ELEMENT, gl.DYNAMIC_DRAW);
    }
    gl.bufferSubData(gl.ARRAY_BUFFER, 0, data);
  }
}

function playerQuad(world: WorldStore, index: number): MeshQuad | null {
  const entity = world.player(index);
  if (entity === undefined) {
    return null;
  }
  return {
    x: entity.x + 0.25,
    y: entity.y + 0.25,
    width: 0.5,
    height: 0.5,
    color: [0.95, 0.35, 0.3, 1],
  };
}

function createProgram(gl: WebGL2RenderingContext, vertex: string, fragment: string): WebGLProgram {
  const program = requireProgram(gl.createProgram());
  const vertexShader = compile(gl, gl.VERTEX_SHADER, vertex);
  const fragmentShader = compile(gl, gl.FRAGMENT_SHADER, fragment);
  gl.attachShader(program, vertexShader);
  gl.attachShader(program, fragmentShader);
  gl.linkProgram(program);
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
    throw new Error(`shader link failed: ${gl.getProgramInfoLog(program) ?? "unknown"}`);
  }
  gl.deleteShader(vertexShader);
  gl.deleteShader(fragmentShader);
  return program;
}

function compile(gl: WebGL2RenderingContext, type: number, source: string): WebGLShader {
  const shader = gl.createShader(type);
  if (shader === null) {
    throw new Error("could not create shader");
  }
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    throw new Error(`shader compile failed: ${gl.getShaderInfoLog(shader) ?? "unknown"}`);
  }
  return shader;
}

function requireBuffer(buffer: WebGLBuffer | null): WebGLBuffer {
  if (buffer === null) {
    throw new Error("could not create vertex buffer");
  }
  return buffer;
}

function requireProgram(program: WebGLProgram | null): WebGLProgram {
  if (program === null) {
    throw new Error("could not create shader program");
  }
  return program;
}
