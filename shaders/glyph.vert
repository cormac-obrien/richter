#version 450

// vertex rate
layout(location = 0) in vec2 a_position;
layout(location = 1) in vec2 a_texcoord;

// instance rate
layout(location = 2) in vec2 a_instance_position;
layout(location = 3) in vec2 a_instance_scale;
layout(location = 4) in uint a_instance_layer;

layout(location = 0) out vec2 f_texcoord;
layout(location = 1) out uint f_layer;

void main() {
  f_texcoord = a_texcoord;
  f_layer = a_instance_layer;
  gl_Position = vec4(a_instance_scale * a_position + a_instance_position, 0.0, 1.0);
}
