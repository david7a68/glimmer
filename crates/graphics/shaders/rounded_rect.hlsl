#define RS "RootFlags(ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT), \
                       RootConstants(num32BitConstants = 2, b0)"

struct DrawConstants
{
    uint screen_width;
    uint screen_height;
};

// Constants set by the root signature
ConstantBuffer<DrawConstants> draw_constants : register(b0);

struct VsInput
{
    float2 position : POSITION;
    float2 rect_size : RECT_SIZE;
    float2 rect_center: RECT_CENTER;
    float4 outer_radius : OUTER_RADIUS;
    float4 inner_radius : INNER_RADIUS;
    float4 color : COLOR;
};

struct VsOutput
{
    float4 position : SV_POSITION;
    float2 rect_size : RECT_SIZE;
    float2 rect_center : RECT_CENTER;
    float4 outer_radius : OUTER_RADIUS;
    float4 inner_radius : INNER_RADIUS;
    float4 color : COLOR;
};

[RootSignature(RS)]
VsOutput vertex_main(VsInput input)
{
    VsOutput output;
    output.position = float4((input.position.x / draw_constants.screen_width) * 2.0f - 1.0f,
                             ((draw_constants.screen_height - input.position.y) / draw_constants.screen_height) * 2.0f - 1.0f,
                             0.0f, 1.0f);
    output.rect_size = input.rect_size;
    output.rect_center = input.rect_center;
    output.outer_radius = input.outer_radius;
    output.inner_radius = input.inner_radius;
    output.color = input.color;
    return output;
}

float4 pixel_main(VsOutput input) : SV_TARGET
{
    // Compute the position of the pixel relative to the center of the rect.
    float2 position = input.position.xy - input.rect_center;

    // Identify the quadrant of the rect and thus the radius to use, and place
    // it in radius.x;
    float4 radius = input.outer_radius;
    radius.xy = position.x > 0.0 ? radius.xy : radius.zw;
    radius.x  = position.y > 0.0 ? radius.x  : radius.y;

    float2 half_rect = input.rect_size / 2;

    // Ensure that the radius is reasonable (not larger than the rect, or
    // negative).
    radius.x = clamp(radius.x, 0.0f, min(half_rect.x, half_rect.y));

    float2 distance_from_edge = abs(position) - half_rect + radius.x;
    float outside_distance = length(max(distance_from_edge, 0.0));
    float inside_distance = min(max(distance_from_edge.x, distance_from_edge.y), 0.0);
    float distance = inside_distance + outside_distance - radius.x;
    // https://www.shadertoy.com/view/4ssSRl

    float w = 0.5 * fwidth(distance);
    w *= 1.1f;

    float4 color = lerp(float4(0.0, 0.0, 0.0, 0.0), input.color, smoothstep(w, -w, distance));

    return color;
}
