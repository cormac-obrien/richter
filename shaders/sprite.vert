#version 450

layout(location = 0) in vec3 a_position;
layout(location = 1) in vec2 a_diffuse;

layout(location = 0) out vec2 f_diffuse;
layout(location = 1) out vec2 f_lightmap;
layout(location = 2) out uvec4 f_lightmap_anim;

layout(set = 0, binding = 0) uniform FrameUniforms {
  float light_anim_frames[64];
  vec4 camera_pos;
  float time;
} frame_uniforms;

layout(set = 1, binding = 0) uniform EntityUniforms {
  mat4 u_transform;
} entity_uniforms;

void main() {
  f_diffuse = a_diffuse;
  gl_Position = entity_uniforms.u_transform
    * vec4(-a_position.y, a_position.z, -a_position.x, 1.0);

}
