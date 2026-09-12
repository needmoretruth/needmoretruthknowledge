//! The name, drawn large.
//!
//! The big logo needs 40 columns and 6 rows. Terminals that cannot spare the height get the small
//! one instead of a cropped one.

/// Rows of the large logo. Drawn only when the screen can spare the room.
pub const LARGE: [&str; 6] = [
    "███╗   ██╗███╗   ███╗████████╗██╗  ██╗",
    "████╗  ██║████╗ ████║╚══██╔══╝██║ ██╔╝",
    "██╔██╗ ██║██╔████╔██║   ██║   █████╔╝ ",
    "██║╚██╗██║██║╚██╔╝██║   ██║   ██╔═██╗ ",
    "██║ ╚████║██║ ╚═╝ ██║   ██║   ██║  ██╗",
    "╚═╝  ╚═══╝╚═╝     ╚═╝   ╚═╝   ╚═╝  ╚═╝",
];

/// Columns the large logo needs.
pub const LARGE_WIDTH: u16 = 38;
/// Rows the large logo needs.
pub const LARGE_HEIGHT: u16 = 6;
