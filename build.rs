pub fn main() {
    cfg_aliases::cfg_aliases! {
        debug_location: { any(
            all(feature = "debug-location", debug_assertions),
            feature = "debug-location-releases"
        ) }
    }
}
