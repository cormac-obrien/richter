+++
title = "HUD Updates and Timing Bugs"
template = "blog-post.html"
date = 2018-04-26
+++

![HUD Screenshot][1]

The HUD now renders armor, health and current ammo counts in addition to the
per-ammo type display at the top. The latter uses [conchars][2], which, as the
name suggests, are used for rendering text to the in-game console. Now that I
can load and display these I can start working on the console, which ought to
make debugging a great deal easier.

Unfortunately, the client is still plagued by a bug with position lerping that
causes the geometry to jitter back and forth. This is most likely caused by bad
time delta calculations in `Client::update_time()` ([Github][3]), but I haven't
been able to pinpoint the exact problem -- only that the lerp factor seems to go
out of the expected range of `[0, 1)` once per server frame. I'll keep an eye on
it.

[1]: /blog/2018-04-26/hud-screenshot.png
[2]: https://quakewiki.org/wiki/Quake_font
[3]: https://github.com/cormac-obrien/richter/blob/12b1d9448cf9c3cfed013108fe0866cb78755902/src/client/mod.rs#L1499-L1552
