//! In-process WMI queries (COM) so Windows system facts never need a PowerShell child.
//!
//! Every call runs on the caller's thread: use it from a blocking worker. COM is
//! initialised for the duration of the call (multi-threaded apartment) and released
//! again when this thread initialised it.
use serde_json::{Map, Value};
use windows::core::{BSTR, PCWSTR};
use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoSetProxyBlanket, CoUninitialize, CLSCTX_INPROC_SERVER,
    COINIT_MULTITHREADED, EOAC_NONE, RPC_C_AUTHN_LEVEL_CALL, RPC_C_IMP_LEVEL_IMPERSONATE,
};
use windows::Win32::System::Variant::{
    VariantClear, VARENUM, VARIANT, VT_BOOL, VT_BSTR, VT_I1, VT_I2, VT_I4, VT_I8, VT_INT, VT_R4,
    VT_R8, VT_UI1, VT_UI2, VT_UI4, VT_UI8, VT_UINT,
};
use windows::Win32::System::Wmi::{
    IWbemClassObject, IWbemLocator, IWbemServices, WbemLocator, WBEM_FLAG_FORWARD_ONLY,
    WBEM_FLAG_RETURN_IMMEDIATELY, WBEM_INFINITE,
};

/// RPC_C_AUTHN_WINNT / RPC_C_AUTHZ_NONE (rpcdce.h).
const AUTHN_WINNT: u32 = 10;
const AUTHZ_NONE: u32 = 0;

pub(super) struct Com {
    owned: bool,
}
impl Com {
    pub(super) fn init() -> Result<Self, String> {
        let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if hr == RPC_E_CHANGED_MODE {
            // The thread already runs an STA; WMI works there too, but it is not ours to release.
            return Ok(Self { owned: false });
        }
        hr.ok().map_err(|e| format!("COM initialisation failed: {e}"))?;
        Ok(Self { owned: true })
    }
}
impl Drop for Com {
    fn drop(&mut self) {
        if self.owned {
            unsafe { CoUninitialize() };
        }
    }
}

pub(super) struct Wmi {
    services: IWbemServices,
}
impl Wmi {
    /// Connect to a local namespace such as `ROOT\CIMV2`. Requires [`Com::init`] on this thread.
    pub(super) fn connect(namespace: &str) -> Result<Self, String> {
        unsafe {
            let locator: IWbemLocator = CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| format!("WMI locator unavailable: {e}"))?;
            let services = locator
                .ConnectServer(
                    &BSTR::from(namespace),
                    &BSTR::new(),
                    &BSTR::new(),
                    &BSTR::new(),
                    0,
                    &BSTR::new(),
                    None,
                )
                .map_err(|e| format!("WMI namespace {namespace} unavailable: {e}"))?;
            CoSetProxyBlanket(
                &services,
                AUTHN_WINNT,
                AUTHZ_NONE,
                PCWSTR::null(),
                RPC_C_AUTHN_LEVEL_CALL,
                RPC_C_IMP_LEVEL_IMPERSONATE,
                None,
                EOAC_NONE,
            )
            .map_err(|e| format!("WMI security setup failed: {e}"))?;
            Ok(Self { services })
        }
    }

    /// Run a WQL query and return the requested properties of every row.
    pub(super) fn query(&self, wql: &str, fields: &[&str]) -> Result<Vec<Map<String, Value>>, String> {
        let mut rows = Vec::new();
        unsafe {
            let enumerator = self
                .services
                .ExecQuery(
                    &BSTR::from("WQL"),
                    &BSTR::from(wql),
                    WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY,
                    None,
                )
                .map_err(|e| format!("WMI query failed: {e}"))?;
            loop {
                let mut objects: [Option<IWbemClassObject>; 1] = [None];
                let mut returned = 0u32;
                let hr = enumerator.Next(WBEM_INFINITE, &mut objects, &mut returned);
                hr.ok().map_err(|e| format!("WMI enumeration failed: {e}"))?;
                let Some(object) = objects[0].take().filter(|_| returned > 0) else {
                    break;
                };
                let mut row = Map::new();
                for field in fields {
                    let name: Vec<u16> = field.encode_utf16().chain(Some(0)).collect();
                    let mut value = VARIANT::default();
                    let read = object.Get(PCWSTR(name.as_ptr()), 0, &mut value, None, None);
                    let json = if read.is_ok() { variant_json(&value) } else { Value::Null };
                    let _ = VariantClear(&mut value);
                    row.insert((*field).to_owned(), json);
                }
                rows.push(row);
            }
        }
        Ok(rows)
    }
}

/// Scalars only; WMI reports uint64/sint64 as BSTR, which callers parse like PowerShell did.
unsafe fn variant_json(value: &VARIANT) -> Value {
    unsafe {
        let inner = &value.Anonymous.Anonymous;
        let data = &inner.Anonymous;
        let number = |n: f64| serde_json::Number::from_f64(n).map_or(Value::Null, Value::Number);
        match inner.vt {
            VT_BSTR => Value::String(data.bstrVal.to_string()),
            VT_BOOL => Value::Bool(data.boolVal.0 != 0),
            VT_I1 => Value::from(data.cVal),
            VT_UI1 => Value::from(data.bVal),
            VT_I2 => Value::from(data.iVal),
            VT_UI2 => Value::from(data.uiVal),
            VT_I4 | VT_INT => Value::from(data.lVal),
            VT_UI4 | VT_UINT => Value::from(data.ulVal),
            VT_I8 => Value::from(data.llVal),
            VT_UI8 => Value::from(data.ullVal),
            VT_R4 => number(f64::from(data.fltVal)),
            VT_R8 => number(data.dblVal),
            VARENUM(_) => Value::Null,
        }
    }
}

/// Quote a value for a WQL string literal.
pub(super) fn wql_string(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wql_strings_escape_quotes_and_backslashes() {
        assert_eq!(wql_string(r"C:"), "'C:'");
        assert_eq!(wql_string(r"Wi-Fi 'home' \x"), r"'Wi-Fi \'home\' \\x'");
    }
    #[test]
    fn local_wmi_answers_operating_system_query() {
        let _com = Com::init().unwrap();
        let wmi = Wmi::connect(r"ROOT\CIMV2").unwrap();
        let rows = wmi
            .query("SELECT Caption, BuildNumber FROM Win32_OperatingSystem", &["Caption", "BuildNumber"])
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert!(rows[0]["Caption"].as_str().is_some_and(|s| s.contains("Windows")));
        assert!(rows[0]["BuildNumber"].as_str().is_some_and(|s| s.parse::<u32>().is_ok()));
    }
}
