+++
title = "Complications with Cross-Platform Input Handling"
template = "blog-post.html"
date = 2018-07-20
+++

It was bound to happen eventually, but the input handling module is the first
part of the project to display different behavior across platforms. [winit][1]
provides a fairly solid basis for input handling, but Windows and Linux differ
in terms of what sort of event is delivered to the program.

Initially, I used `WindowEvent`s for everything. This works perfectly well for
keystrokes and mouse clicks, but mouse movement may still have acceleration
applied, which is undesirable for camera control. `winit` also offers
`DeviceEvent`s for this purpose. I tried just handling mouse movement with raw
input, keeping all other inputs in `WindowEvent`s, but it seems that handling
`DeviceEvent`s on Linux causes the `WindowEvent`s to be eaten.

The next obvious solution is to simply handle everything with `DeviceEvent`s,
but this presents additional problems. First, Windows doesn't seem to even
deliver keyboard input as a `DeviceEvent` -- keyboard input still needs to be
polled as a `WindowEvent`. It also means that window focus has to be handled
manually, since `DeviceEvent`s are delivered regardless of whether the window
is focused or not.

To add to the complexity of this problem, apparently not all window managers
are well-behaved when it comes to determining focus. I run [i3wm][2] on my
Linux install, and it doesn't deliver `WindowEvent::Focused` events when
toggling focus or switching workspaces. This will have to remain an unsolved
problem for the time being.

[1]: https://github.com/tomaka/winit/
[2]: https://i3wm.org/
