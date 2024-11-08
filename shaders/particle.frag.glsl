#version 450

layout(location = 0) in vec2 f_texcoord;

layout(push_constant) uniform PushConstants {
  layout(offset = 64) uint color;
} push_constants;

layout(set = 0, binding = 0) uniform sampler u_sampler;
layout(set = 0, binding = 1) uniform texture2D u_texture[256];

layout(location = 0) out vec4 diffuse_attachment;
// layout(location = 1) out vec4 normal_attachment;
layout(location = 2) out vec4 light_attachment;

void main() {
  vec4 tex_color = texture(
    sampler2D(u_texture[push_constants.color], u_sampler),
    f_texcoord
  );

  if (tex_color.a == 0.0) {
    discard;
  }

  diffuse_attachment = tex_color;
  light_attachment = vec4(0.25);
}
