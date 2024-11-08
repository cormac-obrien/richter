#version 450
#extension GL_EXT_nonuniform_qualifier : require

layout(location = 0) in vec2 f_texcoord;
layout(location = 1) flat in uint f_layer;

layout(location = 0) out vec4 output_attachment;

layout(set = 0, binding = 0) uniform sampler u_sampler;
layout(set = 0, binding = 1) uniform texture2D u_texture[256];

void main() {
  vec4 color = texture(sampler2D(u_texture[f_layer], u_sampler), f_texcoord);
  if (color.a == 0) {
    discard;
  } else {
    output_attachment = color;
  }
}
