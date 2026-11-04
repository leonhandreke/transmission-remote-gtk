//! gettext, as the C version's glib/gi18n.h: _(), N_(), C_(), Q_() and
//! ngettext(), for the "transmission-remote-gtk" domain.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_ulong};

unsafe extern "C" {
    fn gettext(msgid: *const c_char) -> *mut c_char;
    fn ngettext(msgid: *const c_char, plural: *const c_char, n: c_ulong) -> *mut c_char;
    fn textdomain(domain: *const c_char) -> *mut c_char;
    fn bindtextdomain(domain: *const c_char, dir: *const c_char) -> *mut c_char;
    fn bind_textdomain_codeset(domain: *const c_char, codeset: *const c_char) -> *mut c_char;
}

pub const GETTEXT_PACKAGE: &str = env!("TRG_PACKAGE_NAME");

fn cstring(s: &str) -> CString {
    CString::new(s).unwrap_or_else(|_| CString::new(s.replace('\0', "")).unwrap())
}

/// Binds our domain (and optionally another one, like GTK's) to dir.
pub fn bind(domain: &str, dir: &str) {
    let domain = cstring(domain);
    let dir = cstring(dir);
    unsafe {
        bindtextdomain(domain.as_ptr(), dir.as_ptr());
        bind_textdomain_codeset(domain.as_ptr(), c"UTF-8".as_ptr());
    }
}

pub fn init(locale_dir: &str) {
    bind(GETTEXT_PACKAGE, locale_dir);
    let domain = cstring(GETTEXT_PACKAGE);
    unsafe {
        textdomain(domain.as_ptr());
    }
}

/// _()
pub fn tr(msgid: &str) -> String {
    if msgid.is_empty() {
        return String::new();
    }
    let c = cstring(msgid);
    unsafe { CStr::from_ptr(gettext(c.as_ptr())).to_string_lossy().into_owned() }
}

/// ngettext()
pub fn ntr(msgid: &str, plural: &str, n: i64) -> String {
    let c = cstring(msgid);
    let p = cstring(plural);
    unsafe {
        CStr::from_ptr(ngettext(c.as_ptr(), p.as_ptr(), n.unsigned_abs() as c_ulong))
            .to_string_lossy()
            .into_owned()
    }
}

/// C_(): a message with a context, "context\004msgid" in the catalogue
pub fn ctr(context: &str, msgid: &str) -> String {
    let key = format!("{context}\u{4}{msgid}");
    let translated = tr(&key);
    if translated == key { msgid.to_string() } else { translated }
}

/// Q_(): "context|msgid", the context stripped if it's untranslated
pub fn qtr(msgid: &str) -> String {
    let translated = tr(msgid);
    if translated == msgid {
        match msgid.find('|') {
            Some(i) => msgid[i + 1..].to_string(),
            None => translated,
        }
    } else {
        translated
    }
}

/// N_(): marks a string for translation, translated where it's used
#[macro_export]
macro_rules! N_ {
    ($s:literal) => {
        $s
    };
}

/// printf-like substitution of a translated format string with %s, %d,
/// %'d, %g, %ld, %lld, %u, %i and their positional forms (%1$s ...). The C
/// version's translations use these.
pub fn sprintf(fmt: &str, args: &[&dyn std::fmt::Display]) -> String {
    let mut out = String::with_capacity(fmt.len() + 16);
    let mut chars = fmt.chars().peekable();
    let mut next = 0;
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        if chars.peek() == Some(&'%') {
            chars.next();
            out.push('%');
            continue;
        }
        // positional: digits followed by '$'
        let mut spec = String::new();
        while let Some(&d) = chars.peek() {
            if d.is_ascii_digit()
                || d == '$'
                || d == '\''
                || d == '.'
                || d == '-'
                || d == 'l'
                || d == 'h'
                || d == 'z'
            {
                spec.push(d);
                chars.next();
            } else {
                break;
            }
        }
        let conv = chars.next();
        let index = match spec.find('$') {
            Some(i) => spec[..i].parse::<usize>().ok().map(|n| n.saturating_sub(1)),
            None => None,
        };
        let i = index.unwrap_or_else(|| {
            let i = next;
            next += 1;
            i
        });
        match (conv, args.get(i)) {
            (Some(_), Some(arg)) => out.push_str(&arg.to_string()),
            (Some(c), None) => {
                out.push('%');
                out.push_str(&spec);
                out.push(c);
            }
            (None, _) => out.push('%'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The translations compiled by build.rs are found
    #[test]
    fn translates() {
        unsafe {
            std::env::set_var("LANGUAGE", "de");
            libc::setlocale(libc::LC_ALL, c"en_US.UTF-8".as_ptr());
        }
        init(env!("TRG_LOCALEDIR"));
        assert_ne!(tr("Downloading"), "Downloading");
    }

    #[test]
    fn substitutes() {
        assert_eq!(sprintf("%s of %d (%.1f%%)", &[&"a", &3, &"x"]), "a of 3 (x%)");
        assert_eq!(sprintf("%2$s %1$s", &[&"a", &"b"]), "b a");
        assert_eq!(sprintf("%ld second", &[&5]), "5 second");
    }
}
