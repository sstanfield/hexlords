//! The game's shaders and the pipelines made from them.
//!
//! GLSL 100 (`attribute`, `varying`, `gl_FragColor`): it runs on OpenGL 2.1 and
//! up, GLES 2 and WebGL 1, and it is what Turbo Ferret's miniquad renderer uses.
//! Only miniquad's OpenGL backend is supported.
//!
//! The C++ gave each hex its place through uniforms (`center`, `widthFactor`,
//! `heightFactor`) and drew it with its own call. Here the positions are worked
//! out on the CPU and arrive in normalized device coordinates, so that a whole
//! layer of the map is one draw call; the border color and width are vertex
//! attributes for the same reason.
//!
//! C++: assets/shaders/es_2.0/ (Hex, HexOverlay, HexMini, MiniLoc, TexPass),
//! src/render/shaders.cpp

use miniquad::{
    BlendFactor, BlendState, BlendValue, BufferLayout, Equation, Pipeline, PipelineParams, RenderingBackend,
    ShaderError, ShaderMeta, ShaderSource, UniformBlockLayout, VertexAttribute, VertexFormat,
};

/// A vertex of a textured hex with a border.
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct HexVertex {
    /// Normalized device coordinates in the widget's viewport.
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    /// (center weight, rim weight); see `hex_mesh::HEX_BARY`.
    pub bary: [f32; 2],
    /// The border's color (rgb) and the rim weight where it starts (a).
    pub border: [f32; 4],
}

/// A vertex of a flat-colored shape.
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct FlatVertex {
    /// Normalized device coordinates in the current viewport.
    pub pos: [f32; 2],
    /// Straight (not premultiplied) rgba.
    pub color: [f32; 4],
}

/// A vertex of a textured quad.
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct BlitVertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
}

/// C++: HexVertex.glsl
const HEX_VERTEX: &str = r#"#version 100
attribute vec2 in_pos;
attribute vec2 in_uv;
attribute vec2 in_bary;
attribute vec4 in_border;
varying vec2 v_uv;
varying vec2 v_bary;
varying vec4 v_border;
void main() {
    gl_Position = vec4(in_pos, 0.0, 1.0);
    v_uv = in_uv;
    v_bary = in_bary;
    v_border = in_border;
}
"#;

/// The tile's texture, or the border color where the fragment is near the rim.
/// C++: HexFragment.glsl
const HEX_FRAGMENT: &str = r#"#version 100
precision mediump float;
varying vec2 v_uv;
varying vec2 v_bary;
varying vec4 v_border;
uniform sampler2D tex;
void main() {
    if (v_bary.y > v_border.a) {
        gl_FragColor = vec4(v_border.rgb, 1.0);
    } else {
        gl_FragColor = texture2D(tex, v_uv);
    }
}
"#;

/// C++: HexOverlayVertex.glsl, HexMiniVertex.glsl, MiniLocVertex.glsl
const FLAT_VERTEX: &str = r#"#version 100
attribute vec2 in_pos;
attribute vec4 in_color;
varying vec4 v_color;
void main() {
    gl_Position = vec4(in_pos, 0.0, 1.0);
    v_color = in_color;
}
"#;

/// C++: HexOverlayFragment.glsl, HexMiniFragment.glsl, MiniLocFragment.glsl
const FLAT_FRAGMENT: &str = r#"#version 100
precision mediump float;
varying vec4 v_color;
void main() {
    gl_FragColor = v_color;
}
"#;

/// C++: TexPassVertex.glsl
const BLIT_VERTEX: &str = r#"#version 100
attribute vec2 in_pos;
attribute vec2 in_uv;
varying vec2 v_uv;
void main() {
    gl_Position = vec4(in_pos, 0.0, 1.0);
    v_uv = in_uv;
}
"#;

/// C++: TexPassFragment.glsl
const BLIT_FRAGMENT: &str = r#"#version 100
precision mediump float;
varying vec2 v_uv;
uniform sampler2D tex;
void main() {
    gl_FragColor = texture2D(tex, v_uv);
}
"#;

/// The game's three pipelines.
pub struct Pipelines {
    /// Textured hexes with borders (`HexVertex`, one texture), alpha blended.
    pub hex: Pipeline,
    /// Flat-colored shapes (`FlatVertex`), alpha blended.
    pub flat: Pipeline,
    /// A textured quad (`BlitVertex`, one texture), not blended.
    pub blit: Pipeline,
}

impl Pipelines {
    /// Compile the shaders and make the pipelines.
    pub fn new(ctx: &mut dyn RenderingBackend) -> Result<Pipelines, ShaderError> {
        let meta = |textured: bool| ShaderMeta {
            uniforms: UniformBlockLayout { uniforms: vec![] },
            images: if textured { vec!["tex".to_string()] } else { vec![] },
        };
        // C++: main.cpp glBlendFunc(GL_SRC_ALPHA, GL_ONE_MINUS_SRC_ALPHA)
        let blend = BlendState::new(
            Equation::Add,
            BlendFactor::Value(BlendValue::SourceAlpha),
            BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
        );
        let blended = PipelineParams { color_blend: Some(blend), ..Default::default() };

        let hex = ctx.new_shader(ShaderSource::Glsl { vertex: HEX_VERTEX, fragment: HEX_FRAGMENT }, meta(true))?;
        let hex = ctx.new_pipeline(
            &[BufferLayout::default()],
            &[
                VertexAttribute::new("in_pos", VertexFormat::Float2),
                VertexAttribute::new("in_uv", VertexFormat::Float2),
                VertexAttribute::new("in_bary", VertexFormat::Float2),
                VertexAttribute::new("in_border", VertexFormat::Float4),
            ],
            hex,
            blended,
        );

        let flat = ctx.new_shader(ShaderSource::Glsl { vertex: FLAT_VERTEX, fragment: FLAT_FRAGMENT }, meta(false))?;
        let flat = ctx.new_pipeline(
            &[BufferLayout::default()],
            &[
                VertexAttribute::new("in_pos", VertexFormat::Float2),
                VertexAttribute::new("in_color", VertexFormat::Float4),
            ],
            flat,
            blended,
        );

        let blit = ctx.new_shader(ShaderSource::Glsl { vertex: BLIT_VERTEX, fragment: BLIT_FRAGMENT }, meta(true))?;
        let blit = ctx.new_pipeline(
            &[BufferLayout::default()],
            &[
                VertexAttribute::new("in_pos", VertexFormat::Float2),
                VertexAttribute::new("in_uv", VertexFormat::Float2),
            ],
            blit,
            PipelineParams::default(),
        );
        Ok(Pipelines { hex, flat, blit })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertices_are_packed_floats() {
        // The pipelines' attribute lists describe these layouts.
        assert_eq!(std::mem::size_of::<HexVertex>(), 10 * 4);
        assert_eq!(std::mem::size_of::<FlatVertex>(), 6 * 4);
        assert_eq!(std::mem::size_of::<BlitVertex>(), 4 * 4);
    }

    #[test]
    fn the_shaders_are_glsl_100() {
        for src in [HEX_VERTEX, HEX_FRAGMENT, FLAT_VERTEX, FLAT_FRAGMENT, BLIT_VERTEX, BLIT_FRAGMENT] {
            assert!(src.starts_with("#version 100\n"));
            assert!(!src.contains("texture(") && !src.contains(" out ") && !src.contains(" in "), "{src}");
        }
        for src in [HEX_FRAGMENT, FLAT_FRAGMENT, BLIT_FRAGMENT] {
            assert!(src.contains("precision mediump float;") && src.contains("gl_FragColor"));
        }
    }
}
