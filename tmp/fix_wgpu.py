import re
from pathlib import Path

root = Path(__file__).resolve().parents[1]
files = [
    root / "src" / "audio" / "pipeline.rs",
    root / "src" / "video" / "pipeline.rs",
]


def fix_text(text: str) -> str:
    # wgpu 28: entry points are now Option<&str>.
    text = text.replace('entry_point: "vs_main",', 'entry_point: Some("vs_main"),')
    text = text.replace('entry_point: "fs_main",', 'entry_point: Some("fs_main"),')

    # wgpu 28: anisotropy_clamp became Option<u8> in the sampler descriptor.
    text = re.sub(
        r"^(?P<indent>\s*)anisotropy_clamp: (?!\()(?P<value>\d+),\s*$",
        r"\g<indent>anisotropy_clamp: Some(\g<value>),",
        text,
        flags=re.MULTILINE,
    )

    # wgpu 28: TextureViewDescriptor now has a required `usage` field.
    text = re.sub(
        r"^(?P<indent>\s*)array_layer_count: None,(?!\n\s*usage)",
        r"\g<indent>array_layer_count: None,\n\g<indent>usage: None,",
        text,
        flags=re.MULTILINE,
    )

    # wgpu 28: RenderPassColorAttachment now has a required `depth_slice` field.
    text = re.sub(
        r"^(?P<indent>\s*)view: target,(?!\n\s*depth_slice)",
        r"\g<indent>view: target,\n\g<indent>depth_slice: None,",
        text,
        flags=re.MULTILINE,
    )

    # wgpu 28: RenderPassDescriptor now has a required `multiview_mask` field.
    text = re.sub(
        r"^(?P<indent>\s*)occlusion_query_set: None,(?!\n\s*multiview_mask)",
        r"\g<indent>occlusion_query_set: None,\n\g<indent>multiview_mask: None,",
        text,
        flags=re.MULTILINE,
    )

    # wgpu 28: render pipeline multiview field is `multiview_mask`.
    text = re.sub(
        r"^(?P<indent>\s*)multiview: None,\s*$",
        r"\g<indent>multiview_mask: None,",
        text,
        flags=re.MULTILINE,
    )

    return text


for path in files:
    original = path.read_text()
    updated = fix_text(original)
    if updated != original:
        path.write_text(updated)
        print(f"Fixed: {path}")
    else:
        print(f"No changes: {path}")
