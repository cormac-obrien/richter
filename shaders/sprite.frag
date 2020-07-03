#version 450

layout(location = 0) in vec2 f_diffuse;
layout(location = 1) in vec2 f_lightmap;

// set 0: per-frame
layout(set = 0, binding = 0) uniform FrameUniforms {
  float light_anim_frames[64];
  vec4 camera_pos;
  float time;
} frame_uniforms;

// set 1: per-entity
layout(set = 1, binding = 1) uniform sampler u_diffuse_sampler;

// set 2: per-texture chain
layout(set = 2, binding = 0) uniform texture2D u_diffuse_texture;

layout(location = 0) out vec4 color_attachment;

void main() {
  vec4 base_color = texture(sampler2D(u_diffuse_texture, u_diffuse_sampler), f_diffuse);
  color_attachment = base_color;
}
