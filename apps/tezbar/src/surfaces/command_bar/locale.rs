//! OS locale presentation; no locale inference from environment variables and no JS engine.
use tezbar_core::instant::currency::AmountFormatter;
#[cfg(not(target_os = "macos"))]
use tezbar_core::instant::transform::{EnglishFormatter, PresentationFormatter, RelativeTime};
pub struct NativeLocale;
#[cfg(target_os = "macos")]
impl AmountFormatter for NativeLocale {
    fn currency(&self, value: f64, code: &str, max: u8) -> String {
        use objc2_foundation::{
            NSLocale, NSNumber, NSNumberFormatter, NSNumberFormatterStyle, NSString,
        };
        objc2::rc::autoreleasepool(|_| {
            let formatter = NSNumberFormatter::new();
            formatter.setLocale(Some(&NSLocale::currentLocale()));
            formatter.setNumberStyle(NSNumberFormatterStyle::CurrencyStyle);
            formatter.setCurrencyCode(Some(&NSString::from_str(code)));
            formatter.setMaximumFractionDigits(max as usize);
            formatter
                .stringFromNumber(&NSNumber::numberWithDouble(value))
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("{} {code}", tezbar_core::instant::number(value)))
        })
    }
}
#[cfg(target_os = "windows")]
impl AmountFormatter for NativeLocale {
    fn currency(&self, value: f64, code: &str, max: u8) -> String {
        // Windows' system ICU implements the same CLDR currency placement and symbols as Intl.
        // Windows 10+ is the supported application baseline. No new runtime DLL is shipped.
        #[link(name = "icu")]
        unsafe extern "C" {
            fn unum_open(
                style: i32,
                pattern: *const u16,
                len: i32,
                locale: *const std::ffi::c_char,
                error: *mut std::ffi::c_void,
                status: *mut i32,
            ) -> *mut std::ffi::c_void;
            fn unum_close(format: *mut std::ffi::c_void);
            fn unum_setTextAttribute(
                format: *mut std::ffi::c_void,
                tag: i32,
                text: *const u16,
                len: i32,
                status: *mut i32,
            );
            fn unum_setAttribute(format: *mut std::ffi::c_void, tag: i32, value: i32);
            fn unum_formatDouble(
                format: *const std::ffi::c_void,
                value: f64,
                result: *mut u16,
                length: i32,
                position: *mut std::ffi::c_void,
                status: *mut i32,
            ) -> i32;
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetUserDefaultLocaleName(name: *mut u16, len: i32) -> i32;
        }
        let mut locale = [0u16; 85];
        let count = unsafe { GetUserDefaultLocaleName(locale.as_mut_ptr(), 85) };
        let name =
            String::from_utf16_lossy(&locale[..count.saturating_sub(1) as usize]).replace('-', "_");
        let name = std::ffi::CString::new(name).expect("OS locale contains no NUL");
        let mut status = 0;
        let format = unsafe {
            unum_open(
                2,
                std::ptr::null(),
                0,
                name.as_ptr(),
                std::ptr::null_mut(),
                &mut status,
            )
        };
        if status > 0 || format.is_null() {
            return format!("{} {code}", tezbar_core::instant::number(value));
        }
        let code = code.encode_utf16().collect::<Vec<_>>();
        let mut out = [0u16; 256];
        let length = unsafe {
            unum_setTextAttribute(format, 5, code.as_ptr(), code.len() as i32, &mut status);
            unum_setAttribute(format, 6, max as i32);
            let n = unum_formatDouble(
                format,
                value,
                out.as_mut_ptr(),
                256,
                std::ptr::null_mut(),
                &mut status,
            );
            unum_close(format);
            n
        };
        if status <= 0 && length >= 0 && (length as usize) <= out.len() {
            String::from_utf16_lossy(&out[..length as usize])
        } else {
            tezbar_core::instant::number(value)
        }
    }
}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
impl AmountFormatter for NativeLocale {
    fn currency(&self, value: f64, code: &str, _: u8) -> String {
        format!("{} {code}", tezbar_core::instant::number(value))
    }
}

/// Windows presentation through the OS ICU (`icu.dll`, Windows 10 1903+), the same CLDR data
/// the retired renderer reached through `Intl`. Any ICU failure falls back to English text.
#[cfg(target_os = "windows")]
pub(crate) mod windows_icu {
    use std::ffi::{CString, c_char, c_void};
    #[link(name = "icu")]
    unsafe extern "C" {
        fn udat_open(
            time_style: i32,
            date_style: i32,
            locale: *const c_char,
            tz_id: *const u16,
            tz_len: i32,
            pattern: *const u16,
            pattern_len: i32,
            status: *mut i32,
        ) -> *mut c_void;
        fn udat_format(
            format: *const c_void,
            date: f64,
            result: *mut u16,
            capacity: i32,
            position: *mut c_void,
            status: *mut i32,
        ) -> i32;
        fn udat_close(format: *mut c_void);
        fn ureldatefmt_open(
            locale: *const c_char,
            number_format: *mut c_void,
            width: i32,
            capitalization: i32,
            status: *mut i32,
        ) -> *mut c_void;
        fn ureldatefmt_format(
            formatter: *const c_void,
            offset: f64,
            unit: i32,
            result: *mut u16,
            capacity: i32,
            status: *mut i32,
        ) -> i32;
        fn ureldatefmt_close(formatter: *mut c_void);
        fn unum_open(
            style: i32,
            pattern: *const u16,
            len: i32,
            locale: *const c_char,
            error: *mut c_void,
            status: *mut i32,
        ) -> *mut c_void;
        fn unum_formatInt64(
            format: *const c_void,
            number: i64,
            result: *mut u16,
            capacity: i32,
            position: *mut c_void,
            status: *mut i32,
        ) -> i32;
        fn unum_close(format: *mut c_void);
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetUserDefaultLocaleName(name: *mut u16, len: i32) -> i32;
    }
    const UDAT_MEDIUM: i32 = 2;
    const UDAT_STYLE_LONG: i32 = 0;
    const UDISPCTX_CAPITALIZATION_NONE: i32 = 1 << 8;
    const UNUM_DECIMAL: i32 = 1;

    pub(crate) fn user_locale() -> String {
        let mut locale = [0u16; 85];
        let count = unsafe { GetUserDefaultLocaleName(locale.as_mut_ptr(), 85) };
        String::from_utf16_lossy(&locale[..count.saturating_sub(1).max(0) as usize])
            .replace('-', "_")
    }
    fn c_locale(locale: &str) -> Option<CString> {
        CString::new(locale).ok()
    }
    /// ICU success includes warnings (negative status); errors are positive.
    fn finish(status: i32, length: i32, out: &[u16]) -> Option<String> {
        (status <= 0 && length >= 0 && (length as usize) <= out.len())
            .then(|| String::from_utf16_lossy(&out[..length as usize]))
            .filter(|text| !text.is_empty())
    }
    /// Medium date + medium time, matching the macOS `NSDateFormatter` presentation.
    pub(crate) fn timestamp(locale: &str, time_zone: Option<&str>, ms: i64) -> Option<String> {
        let locale = c_locale(locale)?;
        let zone: Option<Vec<u16>> = time_zone.map(|zone| zone.encode_utf16().collect());
        let (zone_ptr, zone_len) = zone
            .as_ref()
            .map_or((std::ptr::null(), -1), |zone| (zone.as_ptr(), zone.len() as i32));
        let mut status = 0;
        let format = unsafe {
            udat_open(
                UDAT_MEDIUM,
                UDAT_MEDIUM,
                locale.as_ptr(),
                zone_ptr,
                zone_len,
                std::ptr::null(),
                -1,
                &mut status,
            )
        };
        if status > 0 || format.is_null() {
            return None;
        }
        let mut out = [0u16; 256];
        let length = unsafe {
            let n = udat_format(
                format,
                ms as f64,
                out.as_mut_ptr(),
                out.len() as i32,
                std::ptr::null_mut(),
                &mut status,
            );
            udat_close(format);
            n
        };
        finish(status, length, &out)
    }
    /// Named relative phrases ("yesterday", "last week"), like `Intl` `numeric: "auto"`.
    pub(crate) fn relative(locale: &str, unit: &str, value: i64) -> Option<String> {
        let unit = match unit {
            "year" => 0,
            "quarter" => 1,
            "month" => 2,
            "week" => 3,
            "day" => 4,
            "hour" => 5,
            "minute" => 6,
            "second" => 7,
            _ => return None,
        };
        let locale = c_locale(locale)?;
        let mut status = 0;
        let formatter = unsafe {
            ureldatefmt_open(
                locale.as_ptr(),
                std::ptr::null_mut(),
                UDAT_STYLE_LONG,
                UDISPCTX_CAPITALIZATION_NONE,
                &mut status,
            )
        };
        if status > 0 || formatter.is_null() {
            return None;
        }
        let mut out = [0u16; 256];
        let length = unsafe {
            let n = ureldatefmt_format(
                formatter,
                value as f64,
                unit,
                out.as_mut_ptr(),
                out.len() as i32,
                &mut status,
            );
            ureldatefmt_close(formatter);
            n
        };
        finish(status, length, &out)
    }
    pub(crate) fn count(locale: &str, count: usize) -> Option<String> {
        let locale = c_locale(locale)?;
        let value = i64::try_from(count).ok()?;
        let mut status = 0;
        let format = unsafe {
            unum_open(
                UNUM_DECIMAL,
                std::ptr::null(),
                0,
                locale.as_ptr(),
                std::ptr::null_mut(),
                &mut status,
            )
        };
        if status > 0 || format.is_null() {
            return None;
        }
        let mut out = [0u16; 128];
        let length = unsafe {
            let n = unum_formatInt64(
                format,
                value,
                out.as_mut_ptr(),
                out.len() as i32,
                std::ptr::null_mut(),
                &mut status,
            );
            unum_close(format);
            n
        };
        finish(status, length, &out)
    }
}
#[cfg(target_os = "windows")]
impl PresentationFormatter for NativeLocale {
    fn local_timestamp(&self, ms: i64) -> String {
        windows_icu::timestamp(&windows_icu::user_locale(), None, ms).unwrap_or_else(|| {
            EnglishFormatter.local_timestamp(ms)
        })
    }
    fn relative(&self, r: &RelativeTime) -> String {
        windows_icu::relative(&windows_icu::user_locale(), r.unit, r.value)
            .unwrap_or_else(|| EnglishFormatter.relative(r))
    }
    fn count(&self, count: usize) -> String {
        windows_icu::count(&windows_icu::user_locale(), count)
            .unwrap_or_else(|| EnglishFormatter.count(count))
    }
}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
impl PresentationFormatter for NativeLocale {
    fn local_timestamp(&self, ms: i64) -> String {
        EnglishFormatter.local_timestamp(ms)
    }
    fn relative(&self, r: &RelativeTime) -> String {
        EnglishFormatter.relative(r)
    }
    fn count(&self, count: usize) -> String {
        EnglishFormatter.count(count)
    }
}
#[cfg(all(test, target_os = "windows"))]
mod windows_icu_tests {
    use super::windows_icu;
    // Explicit locale/zone keep the OS ICU check deterministic on any user profile.
    #[::core::prelude::v1::test]
    fn windows_icu_formats_cldr_presentation_for_explicit_locale() {
        assert_eq!(windows_icu::count("en_US", 12345).as_deref(), Some("12,345"));
        assert_eq!(windows_icu::relative("en_US", "week", -1).as_deref(), Some("last week"));
        assert_eq!(windows_icu::relative("en_US", "day", 1).as_deref(), Some("tomorrow"));
        assert_eq!(windows_icu::relative("en_US", "second", 0).as_deref(), Some("now"));
        assert_eq!(windows_icu::relative("en_US", "hour", -3).as_deref(), Some("3 hours ago"));
        assert!(windows_icu::relative("en_US", "fortnight", 1).is_none());
        let stamp = windows_icu::timestamp("en_US", Some("UTC"), 0).expect("ICU medium timestamp");
        assert!(stamp.starts_with("Jan 1, 1970"), "{stamp}");
        assert!(stamp.contains("12:00:00"), "{stamp}");
        assert!(!windows_icu::user_locale().is_empty());
    }
}
#[cfg(target_os = "macos")]
impl tezbar_core::instant::transform::PresentationFormatter for NativeLocale {
    fn local_timestamp(&self, ms: i64) -> String {
        use objc2_foundation::{NSDate, NSDateFormatter, NSDateFormatterStyle, NSLocale};
        objc2::rc::autoreleasepool(|_| {
            let f = NSDateFormatter::new();
            f.setLocale(Some(&NSLocale::currentLocale()));
            f.setDateStyle(NSDateFormatterStyle::MediumStyle);
            f.setTimeStyle(NSDateFormatterStyle::MediumStyle);
            f.stringFromDate(&NSDate::dateWithTimeIntervalSince1970(ms as f64 / 1000.))
                .to_string()
        })
    }
    fn relative(&self, r: &tezbar_core::instant::transform::RelativeTime) -> String {
        use objc2_foundation::{
            NSCalendar, NSCalendarOptions, NSCalendarUnit, NSDate, NSLocale,
            NSRelativeDateTimeFormatter, NSRelativeDateTimeFormatterStyle,
        };
        objc2::rc::autoreleasepool(|_| {
            let calendar = NSCalendar::currentCalendar();
            let f = NSRelativeDateTimeFormatter::new();
            f.setLocale(Some(&NSLocale::currentLocale()));
            f.setCalendar(Some(&calendar));
            f.setDateTimeStyle(NSRelativeDateTimeFormatterStyle::Named);
            let unit = match r.unit {
                "year" => NSCalendarUnit::Year,
                "month" => NSCalendarUnit::Month,
                "week" => NSCalendarUnit::WeekOfYear,
                "day" => NSCalendarUnit::Day,
                "hour" => NSCalendarUnit::Hour,
                "minute" => NSCalendarUnit::Minute,
                _ => NSCalendarUnit::Second,
            };
            let now = NSDate::new();
            let date = calendar
                .dateByAddingUnit_value_toDate_options(
                    unit,
                    r.value as isize,
                    &now,
                    NSCalendarOptions::empty(),
                )
                .unwrap_or_else(|| now.clone());
            f.localizedStringForDate_relativeToDate(&date, &now)
                .to_string()
        })
    }
    fn count(&self, count: usize) -> String {
        use objc2_foundation::{NSLocale, NSNumber, NSNumberFormatter, NSNumberFormatterStyle};
        objc2::rc::autoreleasepool(|_| {
            let f = NSNumberFormatter::new();
            f.setLocale(Some(&NSLocale::currentLocale()));
            f.setNumberStyle(NSNumberFormatterStyle::DecimalStyle);
            f.setMaximumFractionDigits(0);
            f.stringFromNumber(&NSNumber::numberWithUnsignedLongLong(count as u64))
                .map(|s| s.to_string())
                .unwrap_or_else(|| count.to_string())
        })
    }
}
