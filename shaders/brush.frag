#version 450
#define LIGHTMAP_ANIM_END (255)

const uint TEXTURE_KIND_REGULAR = 0;
const uint TEXTURE_KIND_WARP = 1;
const uint TEXTURE_KIND_SKY = 2;

const float WARP_AMPLITUDE = 0.15;
const float WARP_FREQUENCY = 0.25;
const float WARP_SCALE = 1.0;

layout(location = 0) in vec3 f_normal;
layout(location = 1) in vec2 f_diffuse; // also used for fullbright
layout(location = 2) in vec2 f_lightmap;
flat layout(location = 3) in uvec4 f_lightmap_anim;

// set 0: per-frame
layout(set = 0, binding = 0) uniform FrameUniforms {
    uint light_anim_frames[64]; // range [0, 550]
    vec4 camera_pos;
    float time;
    bool r_lightmap;
} frame_uniforms;

// set 1: per-entity
layout(set = 1, binding = 1) uniform sampler u_diffuse_sampler; // also used for fullbright
layout(set = 1, binding = 2) uniform sampler u_lightmap_sampler;

// set 2: per-texture
layout(set = 2, binding = 0) uniform texture2D u_diffuse_texture;
layout(set = 2, binding = 1) uniform texture2D u_fullbright_texture;
layout(set = 2, binding = 2) uniform TextureUniforms {
    uint kind;
} texture_uniforms;

// set 3: per-face
layout(set = 3, binding = 0) uniform texture2D u_lightmap_texture[4];

layout(location = 0) out vec4 diffuse_attachment;
layout(location = 1) out vec4 normal_attachment;

vec4 blend_light(vec4 color) {
    float light = 0.0;
    for (int i = 0; i < 4 && f_lightmap_anim[i] != LIGHTMAP_ANIM_END; i++) {
        uint umap = uint(texture(
            sampler2D(u_lightmap_texture[i], u_lightmap_sampler),
            f_lightmap
        ).r * 255.0) * 2;
        uint ustyle = frame_uniforms.light_anim_frames[f_lightmap_anim[i]];
        uint ulight = umap * ustyle;
        light += float(min(ulight >> 8, 255)) / 256.0;
    }

    if (frame_uniforms.r_lightmap) {
        return vec4(light.rrr, 1.0);
    } else {
        return vec4(color.rgb * light.rrr, 1.0);
    }
}

void main() {
    switch (texture_uniforms.kind) {
        case TEXTURE_KIND_REGULAR:
            vec4 base_color = texture(
                sampler2D(u_diffuse_texture, u_diffuse_sampler),
                f_diffuse
            );

            float fullbright = texture(
                sampler2D(u_fullbright_texture, u_diffuse_sampler),
                f_diffuse
            ).r;

            if (fullbright != 0.0) {
                diffuse_attachment = base_color;
            } else {
                diffuse_attachment = blend_light(base_color);
            }
            break;

        case TEXTURE_KIND_WARP:
            // note the texcoord transpose here
            vec2 wave1 = 3.14159265359
                * (WARP_SCALE * f_diffuse.ts
                    + WARP_FREQUENCY * frame_uniforms.time);

            vec2 warp_texcoord = f_diffuse.st + WARP_AMPLITUDE
                * vec2(sin(wave1.s), sin(wave1.t));

            diffuse_attachment = texture(
                sampler2D(u_diffuse_texture, u_diffuse_sampler),
                warp_texcoord
            );
            break;

        case TEXTURE_KIND_SKY:
            vec2 base = mod(f_diffuse + frame_uniforms.time, 1.0);
            vec2 cloud_texcoord = vec2(base.s * 0.5, base.t);
            vec2 sky_texcoord = vec2(base.s * 0.5 + 0.5, base.t);

            vec4 sky_color = texture(
                sampler2D(u_diffuse_texture, u_diffuse_sampler),
                sky_texcoord
            );
            vec4 cloud_color = texture(
                sampler2D(u_diffuse_texture, u_diffuse_sampler),
                cloud_texcoord
            );

            // 0.0 if black, 1.0 otherwise
            float cloud_factor;
            if (cloud_color.r + cloud_color.g + cloud_color.b == 0.0) {
                cloud_factor = 0.0;
            } else {
                cloud_factor = 1.0;
            }
            diffuse_attachment = mix(sky_color, cloud_color, cloud_factor);
            break;

        // not possible
        default:
            break;
    }

    // rescale normal to [0, 1]
    normal_attachment = vec4(f_normal / 2.0 + 0.5, 1.0);
}
