#version 450

const uint TEXTURE_KIND_NORMAL = 0;
const uint TEXTURE_KIND_WARP = 1;
const uint TEXTURE_KIND_SKY = 2;

layout(location = 0) in vec3 a_position;
layout(location = 1) in vec2 a_diffuse;
layout(location = 2) in vec2 a_lightmap;
layout(location = 3) in uvec4 a_lightmap_anim;

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

layout(set = 2, binding = 2) uniform TextureUniforms {
    uint kind;
} texture_uniforms;

void main() {
    if (texture_uniforms.kind == TEXTURE_KIND_SKY) {
        vec3 dir = a_position - frame_uniforms.camera_pos.xyz;
        dir.z *= 3.0;

        // the coefficients here are magic taken from the Quake source
        float len = 6.0 * 63.0 / length(dir);
        dir = vec3(dir.xy * len, dir.z);
        f_diffuse = (mod(8.0 * frame_uniforms.time, 128.0) + dir.xy) / 128.0;
    } else {
        f_diffuse = a_diffuse;
    }

    f_lightmap = a_lightmap;
    f_lightmap_anim = a_lightmap_anim;
    gl_Position = entity_uniforms.u_transform * vec4(-a_position.y, a_position.z, -a_position.x, 1.0);

}
