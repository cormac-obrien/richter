#version 450

layout(location = 0) in vec2 f_texcoord;

layout(location = 0) out vec4 color_attachment;

layout(set = 0, binding = 0) uniform sampler u_sampler;
layout(set = 0, binding = 1) uniform texture2D u_color;

void main() {
  color_attachment = texture(sampler2D(u_color, u_sampler), f_texcoord);
}
