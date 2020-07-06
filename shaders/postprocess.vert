#version 450

layout(location = 0) in vec2 a_position;
layout(location = 1) in vec2 a_texcoord;

layout(location = 0) out vec2 f_texcoord;

void main() {
  f_texcoord = a_texcoord;
  gl_Position = vec4(a_position * 2.0 - 1.0, 0.0, 1.0);
}
