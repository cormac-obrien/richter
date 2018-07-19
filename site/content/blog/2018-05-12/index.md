+++
title = "Shared Ownership of Rendering Resources"
template = "blog-post.html"
date = 2018-05-12
+++

Among the most challenging design decisions in writing the rendering code has been the issue of
ownership. In order to avoid linking the rendering logic too closely with the data, most of the
rendering is done by separate `Renderer` objects (i.e., to render an `AliasModel`, one must first
create an `AliasRenderer`).

The process of converting on-disk model data to renderable format is fairly complex. Brush models
are stored in a format designed for the Quake software renderer (which Michael Abrash explained
[quite nicely][1]), while alias models have texture oddities that make it difficult to render them
from a vertex buffer. In addition, all textures are composed of 8-bit indices into `gfx/palette.lmp`
and must be converted to RGB in order to upload them to the GPU. Richter interleaves the position
and texture coordinate data before upload.

The real challenge is in determining where to store the objects for resource creation (e.g.
`gfx::Factory`) and the resource handles (e.g. `gfx::handle::ShaderResourceView`). Some of these
objects are model-specific -- a particular texture might belong to one model, and thus can be stored
in that model's `Renderer` -- but others need to be more widely available.

The most obvious example of this is the vertex buffer used for rendering quads. This is conceptually
straightforward, but there are several layers of a renderer that might need this functionality.
The `ConsoleRenderer` needs it in order to render the console background, but also needs a
`GlyphRenderer` to render console output -- and the `GlyphRenderer` needs to be able to render
textured quads. The `ConsoleRenderer` could own the `GlyphRenderer`, but the `HudRenderer` also
needs access to render ammo counts.

This leads to a rather complex network of `Rc`s, where many different objects own the basic building
blocks that make up the rendering system. It isn't bad design *per se*, but it's a little difficult
to follow, and I'm hoping that once I have the renderer fully completed I can refine the architecture
to something more elegant.

[1]: https://www.bluesnews.com/abrash/
