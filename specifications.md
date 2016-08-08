# Specifications for the Original Quake (idTech 2) Engine

### Coordinate Systems

Quake's coordinate system specifies its axes as follows:

- The x-axis specifies depth.
- The y-axis specifies width.
- The z-axis specifies height.

This contrasts with the OpenGL coordinate system, in which:

- The x-axis specifies width.
- The y-axis specifies height.
- The z-axis specifies depth (inverted).

Thus, to convert between the coordinate systems:

          x <-> -z
    Quake y <->  x OpenGL
          z <->  y

           x <->  y
    OpenGL y <->  z Quake
           z <-> -x
