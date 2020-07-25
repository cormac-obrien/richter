#version 450

// if this is changed, it must also be changed in client::entity
const uint MAX_LIGHTS = 32;

layout(location = 0) in vec2 a_texcoord;

layout(set = 0, binding = 0) uniform sampler u_sampler;
layout(set = 0, binding = 1) uniform texture2DMS u_diffuse;
layout(set = 0, binding = 2) uniform texture2DMS u_normal;
layout(set = 0, binding = 3) uniform texture2DMS u_light;
layout(set = 0, binding = 4) uniform texture2DMS u_depth;
layout(set = 0, binding = 5) uniform DeferredUniforms {
  mat4 inv_projection;
  uint light_count;
  uint _pad1;
  uvec2 _pad2;
  vec4 lights[MAX_LIGHTS];
} u_deferred;

layout(location = 0) out vec4 color_attachment;

vec3 dlight_origin(vec4 dlight) {
  return dlight.xyz;
}

float dlight_radius(vec4 dlight) {
  return dlight.w;
}

vec3 reconstruct_position(float depth) {
  float x = a_texcoord.s * 2.0 - 1.0;
  float y = (1.0 - a_texcoord.t) * 2.0 - 1.0;
  vec4 ndc = vec4(x, y, depth, 1.0);
  vec4 view = u_deferred.inv_projection * ndc;
  return view.xyz / view.w;
}

void main() {
  ivec2 dims = textureSize(sampler2DMS(u_diffuse, u_sampler));
  ivec2 texcoord = ivec2(vec2(dims) * a_texcoord);
  vec4 in_color = texelFetch(sampler2DMS(u_diffuse, u_sampler), texcoord, gl_SampleID);

  // scale from [0, 1] to [-1, 1]
  vec3 in_normal = 2.0
    * texelFetch(sampler2DMS(u_normal, u_sampler), texcoord, gl_SampleID).xyz
    - 1.0;

  // scale up by 4.0 (see brush.frag)
  vec4 in_light = 4.0 * texelFetch(sampler2DMS(u_light, u_sampler), texcoord, gl_SampleID);
  float in_depth = texelFetch(sampler2DMS(u_depth, u_sampler), texcoord, gl_SampleID).x;
  vec3 position = reconstruct_position(in_depth);

  vec4 out_color = in_color;

  float light = in_light.x + in_light.y + in_light.z + in_light.w;
  for (uint i = 0; i < u_deferred.light_count && i < MAX_LIGHTS; i++) {
    vec4 dlight = u_deferred.lights[i];
    vec3 dir = normalize(position - dlight_origin(dlight));
    float dist = abs(distance(dlight_origin(dlight), position));
    float radius = dlight_radius(dlight);

    if (dist < radius && dot(dir, in_normal) < 0.0) {
      // linear attenuation
      light += (radius - dist) / radius;
    }
  }

  // allow 200% light saturation
  light = min(light, 4.0);

  color_attachment = vec4(out_color.rgb * light, 1.0);
}
