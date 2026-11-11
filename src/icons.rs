//! Line icons on a 24-point grid, drawn in their element's colour.

use rune::Icon;

macro_rules! icon {
    ($name:ident, $id:literal, $body:literal) => {
        pub static $name: Icon = Icon {
            name: $id,
            view: 24.0,
            body: concat!(
                r#"<g fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">"#,
                $body,
                "</g>"
            ),
        };
    };
}

icon!(LIGHT, "blt-light", r#"<path d="M9 18h6M10 21h4"/><path d="M12 3a6 6 0 0 0-3.9 10.6c.8.7 1.3 1.6 1.4 2.4h5c.1-.8.6-1.7 1.4-2.4A6 6 0 0 0 12 3z"/>"#);
icon!(TEAMS, "blt-teams", r#"<circle cx="9" cy="8" r="3.2"/><path d="M3 19.5c0-3.3 2.7-5.6 6-5.6s6 2.3 6 5.6"/><circle cx="17.2" cy="9" r="2.4"/><path d="M16.4 13.7c2.8 0 4.8 1.9 4.8 4.6"/>"#);
icon!(PALETTE, "blt-palette", r#"<path d="M12 3a9 9 0 1 0 0 18c1 0 1.6-.8 1.6-1.6 0-.5-.2-.9-.5-1.2-.3-.3-.5-.7-.5-1.2 0-.9.7-1.6 1.6-1.6H16a5 5 0 0 0 5-5C21 6.2 17 3 12 3z"/><circle cx="7.5" cy="11" r="1"/><circle cx="10.5" cy="7.2" r="1"/><circle cx="15" cy="7.6" r="1"/>"#);
icon!(SLIDERS, "blt-sliders", r#"<path d="M4 6h9M17 6h3M4 12h3M11 12h9M4 18h11M19 18h1"/><circle cx="15" cy="6" r="2"/><circle cx="9" cy="12" r="2"/><circle cx="17" cy="18" r="2"/>"#);
icon!(HELP, "blt-help", r#"<circle cx="12" cy="12" r="9"/><path d="M9.6 9.4a2.5 2.5 0 1 1 3.4 2.4c-.6.3-1 .8-1 1.5v.6"/><path d="M12 17h.01"/>"#);
icon!(CHECK, "blt-check", r#"<path d="M4.5 12.5l4.8 4.8L19.5 7"/>"#);
icon!(ALERT, "blt-alert", r#"<circle cx="12" cy="12" r="9"/><path d="M12 7.5v5.5M12 16.5h.01"/>"#);
icon!(CHEVRON_DOWN, "blt-chevron-down", r#"<path d="M7 10l5 5 5-5"/>"#);
icon!(CHEVRON_UP, "blt-chevron-up", r#"<path d="M7 14l5-5 5 5"/>"#);
icon!(ZAP, "blt-zap", r#"<path d="M13 2.5L4.5 13.5H12l-1 8 8.5-11H12l1-8z"/>"#);
icon!(FOLDER, "blt-folder", r#"<path d="M3.5 7.5A2 2 0 0 1 5.5 5.5h3.6l2 2.2h7.4a2 2 0 0 1 2 2v7.8a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2z"/>"#);
icon!(COPY, "blt-copy", r#"<rect x="8.5" y="8.5" width="12" height="12" rx="2"/><path d="M15.5 8.5V5.5a2 2 0 0 0-2-2h-8a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h3"/>"#);
icon!(EXTERNAL, "blt-external", r#"<path d="M14 4h6v6M20 4l-9 9"/><path d="M18 14v4a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h4"/>"#);
icon!(RESET, "blt-reset", r#"<path d="M4 12a8 8 0 1 0 2.4-5.7L4 8.5"/><path d="M4 4v4.5h4.5"/>"#);
icon!(SPARKLE, "blt-sparkle", r#"<path d="M12 3.5l1.9 5.1 5.1 1.9-5.1 1.9-1.9 5.1-1.9-5.1L5 10.5l5.1-1.9z"/><path d="M18.5 16.5l.7 1.8 1.8.7-1.8.7-.7 1.8-.7-1.8-1.8-.7 1.8-.7z"/>"#);
icon!(USB, "blt-usb", r#"<rect x="7" y="9" width="10" height="12" rx="2"/><path d="M9 9V3.5h6V9M10.5 5.8h.01M13.5 5.8h.01"/>"#);
