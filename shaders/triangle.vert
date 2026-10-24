
#version 450

layout(push_constant) uniform CameraTransform {
    mat4 transform;
} camera;

layout(location = 0) in vec3 inPosition;
layout(location = 1) in vec3 inColor;

layout(location = 0) out vec3 fragColor;

void main() {
    gl_Position = camera.transform * vec4(inPosition, 1.0);
    fragColor = inColor;
}
