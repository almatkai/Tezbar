"""Opus pass 2: read-only physical-input receipt (system HIDIdleTime), in process.

Same registry property `ioreg -c IOHIDSystem -k HIDIdleTime` prints, read through
IOKit/CoreFoundation instead of spawning `ioreg` (~12.6 ms of separate-process
work per sample). Reads only the idle duration; never event content; no
suppression, no OS-global state change. Each receipt carries monotonic
timestamps taken immediately before and after the registry read, so the last
physical input is bounded to [pre-idle, post-idle] instead of being aligned to a
sample timestamp taken earlier.
"""
import ctypes
import ctypes.util
import time

_iokit = ctypes.CDLL(ctypes.util.find_library('IOKit'))
_cf = ctypes.CDLL(ctypes.util.find_library('CoreFoundation'))
_iokit.IOServiceMatching.restype = ctypes.c_void_p
_iokit.IOServiceMatching.argtypes = [ctypes.c_char_p]
_iokit.IOServiceGetMatchingService.restype = ctypes.c_uint32
_iokit.IOServiceGetMatchingService.argtypes = [ctypes.c_uint32, ctypes.c_void_p]
_iokit.IORegistryEntryCreateCFProperty.restype = ctypes.c_void_p
_iokit.IORegistryEntryCreateCFProperty.argtypes = [ctypes.c_uint32, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_uint32]
_iokit.IOObjectRelease.argtypes = [ctypes.c_uint32]
_cf.CFStringCreateWithCString.restype = ctypes.c_void_p
_cf.CFStringCreateWithCString.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_uint32]
_cf.CFGetTypeID.restype = ctypes.c_ulong
_cf.CFGetTypeID.argtypes = [ctypes.c_void_p]
_cf.CFNumberGetTypeID.restype = ctypes.c_ulong
_cf.CFNumberGetValue.restype = ctypes.c_bool
_cf.CFNumberGetValue.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_void_p]
_cf.CFRelease.argtypes = [ctypes.c_void_p]
_KEY = _cf.CFStringCreateWithCString(None, b'HIDIdleTime', 0x08000100)  # UTF-8, immortal
_SINT64 = 4


def _read_ns():
    service = _iokit.IOServiceGetMatchingService(0, _iokit.IOServiceMatching(b'IOHIDSystem'))
    if not service:
        return None
    try:
        value = _iokit.IORegistryEntryCreateCFProperty(service, _KEY, None, 0)
    finally:
        _iokit.IOObjectRelease(service)
    if not value:
        return None
    try:
        if _cf.CFGetTypeID(value) != _cf.CFNumberGetTypeID():
            return None
        out = ctypes.c_int64()
        return out.value if _cf.CFNumberGetValue(value, _SINT64, ctypes.byref(out)) else None
    finally:
        _cf.CFRelease(value)


def receipt(origin):
    """{pre, post, idle_s} relative to `origin` (time.monotonic() base); idle_s None if unreadable."""
    pre = time.monotonic()
    ns = _read_ns()
    post = time.monotonic()
    return dict(pre=pre-origin, post=post-origin, idle_s=None if ns is None or ns < 0 else ns/1e9)


def _finite(v):
    return type(v) in (int, float) and v == v and v not in (float('inf'), float('-inf'))


def quiet_margin_s(samples, legacy_skew_s=.05):
    """Seconds of verified input absence before the window's first CPU read.

    For each sample the latest physical input happened no later than
    read_upper - idle, where read_upper is the receipt's post timestamp, or, for
    pass-1 receipts that carry only the sample's start time `t`, `t + legacy_skew_s`
    (an assumed bound on the sampler-to-ioreg delay). The window starts at the
    first sample's `t` (its CPU counters are read after `t`). Positive: no input
    from margin seconds before the window through its last receipt. Negative or
    zero: an input may have occurred inside the window. Any missing, non-finite,
    negative, unordered or pre-sample receipt fails closed with ValueError.
    """
    if not samples:
        raise ValueError('Cannot qualify an empty idle window')
    latest, previous = None, None
    for s in samples:
        t = s.get('t')
        if not _finite(t) or t < 0 or (previous is not None and t < previous):
            raise ValueError('Cannot qualify an idle window with invalid/unordered timestamps')
        previous = t
        if 'hid' in s:
            h = s['hid'] if isinstance(s['hid'], dict) else {}
            pre, post, idle = h.get('pre'), h.get('post'), h.get('idle_s')
            if not all(_finite(v) and v >= 0 for v in (pre, post, idle)) or post < pre or pre < t:
                raise ValueError('Cannot qualify an idle window without a valid timed HID receipt')
            upper = post-idle
        else:
            idle = s.get('hid_idle_s')
            if not _finite(idle) or idle < 0:
                raise ValueError('Cannot qualify an idle window without valid timestamp/HID receipts')
            upper = t+legacy_skew_s-idle
        latest = upper if latest is None else max(latest, upper)
    return samples[0]['t']-latest


def classify(samples, guard_s=2., legacy_skew_s=.05):
    """'quiet' only when no input in [start-guard_s, last receipt]; otherwise
    'pre_window_input' (inside the guard band) or 'in_window_input'."""
    margin = quiet_margin_s(samples, legacy_skew_s)
    return dict(margin_s=margin, guard_s=guard_s,
                verdict='quiet' if margin > guard_s else 'pre_window_input' if margin > 0 else 'in_window_input')
