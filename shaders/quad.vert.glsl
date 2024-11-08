#version 450

layout(location = 0) in vec2 a_position;
layout(location = 1) in vec2 a_texcoord;

layout(location = 0) out vec2 f_texcoord;

layout(set = 2, binding = 0) uniform QuadUniforms {
  mat4 transform;
} quad_uniforms;

void main() {
  f_texcoord = a_texcoord;
  gl_Position = quad_uniforms.transform * vec4(a_position, 0.0, 1.0);
}
