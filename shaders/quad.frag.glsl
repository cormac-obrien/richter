#version 450

layout(location = 0) in vec2 f_texcoord;

layout(location = 0) out vec4 color_attachment;

layout(set = 0, binding = 0) uniform sampler quad_sampler;
layout(set = 1, binding = 0) uniform texture2D quad_texture;

void main() {
  vec4 color = texture(sampler2D(quad_texture, quad_sampler), f_texcoord);
  if (color.a == 0) {
    discard;
  } else {
    color_attachment = color;
  }
}
