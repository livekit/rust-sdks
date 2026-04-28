#include <metal_stdlib>
using namespace metal;

struct VertexOut {
    float4 position [[position]];
    float2 tex_coord;
};

struct YuvParams {
    uint format;
    uint full_range;
};

struct OverlayParams {
    float2 origin;
    float2 size;
    float2 drawable_size;
};

vertex VertexOut yuv_vertex(uint vertex_id [[vertex_id]]) {
    float2 positions[3] = {
        float2(-1.0, -1.0),
        float2( 3.0, -1.0),
        float2(-1.0,  3.0),
    };
    float2 tex_coords[3] = {
        float2(0.0, 1.0),
        float2(2.0, 1.0),
        float2(0.0, -1.0),
    };

    VertexOut out;
    out.position = float4(positions[vertex_id], 0.0, 1.0);
    out.tex_coord = tex_coords[vertex_id];
    return out;
}

fragment float4 yuv_fragment(
    VertexOut in [[stage_in]],
    texture2d<float, access::sample> y_tex [[texture(0)]],
    texture2d<float, access::sample> u_tex [[texture(1)]],
    texture2d<float, access::sample> v_tex [[texture(2)]],
    sampler tex_sampler [[sampler(0)]],
    constant YuvParams& params [[buffer(0)]]
) {
    float y = y_tex.sample(tex_sampler, in.tex_coord).r;
    float u;
    float v;

    if (params.format == 1) {
        float2 uv = u_tex.sample(tex_sampler, in.tex_coord).rg;
        u = uv.r;
        v = uv.g;
    } else {
        u = u_tex.sample(tex_sampler, in.tex_coord).r;
        v = v_tex.sample(tex_sampler, in.tex_coord).r;
    }

    if (params.full_range == 0) {
        y = max(0.0, y - (16.0 / 255.0)) * (255.0 / 219.0);
        u = (u - (128.0 / 255.0)) * (255.0 / 224.0);
        v = (v - (128.0 / 255.0)) * (255.0 / 224.0);
    } else {
        u -= 0.5;
        v -= 0.5;
    }

    float3 rgb;
    rgb.r = y + 1.402 * v;
    rgb.g = y - 0.344136 * u - 0.714136 * v;
    rgb.b = y + 1.772 * u;
    return float4(saturate(rgb), 1.0);
}

vertex VertexOut overlay_vertex(
    uint vertex_id [[vertex_id]],
    constant OverlayParams& params [[buffer(0)]]
) {
    float2 corners[4] = {
        float2(0.0, 0.0),
        float2(1.0, 0.0),
        float2(0.0, 1.0),
        float2(1.0, 1.0),
    };

    float2 uv = corners[vertex_id];
    float2 pixel = params.origin + uv * params.size;
    float2 ndc = float2(
        (pixel.x / params.drawable_size.x) * 2.0 - 1.0,
        1.0 - (pixel.y / params.drawable_size.y) * 2.0
    );

    VertexOut out;
    out.position = float4(ndc, 0.0, 1.0);
    out.tex_coord = uv;
    return out;
}

fragment float4 overlay_fragment(
    VertexOut in [[stage_in]],
    texture2d<float, access::sample> overlay_tex [[texture(0)]],
    sampler tex_sampler [[sampler(0)]]
) {
    return overlay_tex.sample(tex_sampler, in.tex_coord);
}
