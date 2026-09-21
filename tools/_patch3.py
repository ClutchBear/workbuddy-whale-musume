import io

ROOT = r"D:\work\demo\workbuddy-pet-native"


def patch(path, pairs):
    p = ROOT + "\\" + path
    s = io.open(p, encoding="utf-8").read()
    for old, new in pairs:
        if old not in s:
            print("  !! 未命中 [%s]: %r" % (path, old[:70]))
            continue
        s = s.replace(old, new, 1)
    io.open(p, "w", encoding="utf-8", newline="").write(s)
    print("  已改 %s" % path)


patch("src/core.rs", [
    ("pub fn achievement_name(id: &str) -> &'static str {",
     "pub fn achievement_name<'a>(id: &'a str) -> &'a str {"),
])

patch("src/render.rs", [
    ("    if let Some(fx) = v.fx {", "    if let Some(fx) = v.fx.as_ref() {"),
])

patch("src/main.rs", [
    ("            biCompression: DIB_RGB_COLORS,\n            ..Default::default()",
     "            biCompression: 0, // BI_RGB\n            ..Default::default()"),
])
print("done")
