#version 450

layout(location = 0) in vec3 a_position;
layout(location = 1) in vec3 a_normal;
layout(location = 2) in vec2 a_diffuse;

layout(location = 0) out vec3 f_normal;
layout(location = 1) out vec2 f_diffuse;

layout(set = 0, binding = 0) uniform FrameUniforms {
  float light_anim_frames[64];
  vec4 camera_pos;
  float time;
} frame_uniforms;

layout(set = 1, binding = 0) uniform EntityUniforms {
  mat4 u_transform;
  mat4 u_model;
} entity_uniforms;

// convert from Quake coordinates
vec3 convert(vec3 from) {
  return vec3(-from.y, from.z, -from.x);
}

void main() {
  f_normal = mat3(transpose(inverse(entity_uniforms.u_model))) * convert(a_normal);
  f_diffuse = a_diffuse;
  gl_Position = entity_uniforms.u_transform
    * vec4(convert(a_position), 1.0);
}
