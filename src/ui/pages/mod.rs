//! One file per page. A page is an `impl SillyBorgApp` block with one method that draws it
//! (`tk::page_head`, then `tk::content` with panels and rows). To add a page, see `Page` in `ui/mod.rs`.

mod app;
mod audio;
mod clips;
mod help;
mod hotkeys;
mod overview;
mod profiles;
mod quality;
mod source;

