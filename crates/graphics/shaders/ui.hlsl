// Root signature with scale/translate

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
    float4 color : COLOR;
};

struct VsOutput
{
    float4 position : SV_POSITION;
    float4 color : COLOR;
};

[RootSignature(RS)]
VsOutput vertex_main(VsInput input)
{
    VsOutput output;

    output.position = float4((input.position.x / draw_constants.screen_width) * 2.0f - 1.0f,
                             ((draw_constants.screen_height - input.position.y) / draw_constants.screen_height) * 2.0f - 1.0f,
                             0.0f, 1.0f);
    output.color = input.color;

    return output;
}

float4 pixel_main(VsOutput input) : SV_TARGET
{
    return input.color;
}
