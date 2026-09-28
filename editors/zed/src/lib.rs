use zed_extension_api as zed;

struct MetaglyphExtension;

impl zed::Extension for MetaglyphExtension {
    fn new() -> Self {
        MetaglyphExtension
    }
}

zed::register_extension!(MetaglyphExtension);
