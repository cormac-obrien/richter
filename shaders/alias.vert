#version 450

layout(location = 0) in vec3 a_position1;
// layout(location = 1) in vec3 a_position2;
layout(location = 2) in vec3 a_normal;
layout(location = 3) in vec2 a_diffuse;

layout(push_constant) uniform PushConstants {
  mat4 transform;
  mat4 model_view;
} push_constants;

layout(location = 0) out vec3 f_normal;
layout(location = 1) out vec2 f_diffuse;

// convert from Quake coordinates
vec3 convert(vec3 from) {
  return vec3(-from.y, from.z, -from.x);
}

void main() {
  f_normal = mat3(transpose(inverse(push_constants.model_view))) * convert(a_normal);
  f_diffuse = a_diffuse;
  gl_Position = push_constants.transform * vec4(convert(a_position1), 1.0);
}
