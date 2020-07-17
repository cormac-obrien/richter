#version 450

layout(location = 0) in vec3 a_position;
layout(location = 1) in vec2 a_texcoord;

layout(push_constant) uniform PushConstants {
  mat4 transform;
} push_constants;

layout(location = 0) out vec2 f_texcoord;

void main() {
  f_texcoord = a_texcoord;
  gl_Position = push_constants.transform * vec4(a_position, 1.0);
}
