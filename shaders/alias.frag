#version 450

layout(location = 0) in vec3 f_normal;
layout(location = 1) in vec2 f_diffuse;

// set 1: per-entity
layout(set = 1, binding = 1) uniform sampler u_diffuse_sampler;

// set 2: per-texture chain
layout(set = 2, binding = 0) uniform texture2D u_diffuse_texture;

layout(location = 0) out vec4 diffuse_attachment;
layout(location = 1) out vec4 normal_attachment;
layout(location = 2) out vec4 light_attachment;

void main() {
  diffuse_attachment = texture(
    sampler2D(u_diffuse_texture, u_diffuse_sampler),
    f_diffuse
  );

  // TODO: get ambient light from uniform
  light_attachment = vec4(1.0, 1.0, 1.0, 1.0);

  // rescale normal to [0, 1]
  normal_attachment = vec4(f_normal / 2.0 + 0.5, 1.0);
}
