//! OS locale presentation; no locale inference from environment variables and no JS engine.
use tezbar_core::instant::currency::AmountFormatter;
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
