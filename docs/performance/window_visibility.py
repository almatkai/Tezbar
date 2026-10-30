"""Read only this benchmark PID's on-screen window bounds, never other titles."""
import ctypes
CG=ctypes.CDLL('/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics')
CF=ctypes.CDLL('/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation')
CG.CGWindowListCopyWindowInfo.argtypes=[ctypes.c_uint32,ctypes.c_uint32]
CG.CGWindowListCopyWindowInfo.restype=ctypes.c_void_p
CF.CFArrayGetCount.argtypes=[ctypes.c_void_p];CF.CFArrayGetCount.restype=ctypes.c_long
CF.CFArrayGetValueAtIndex.argtypes=[ctypes.c_void_p,ctypes.c_long];CF.CFArrayGetValueAtIndex.restype=ctypes.c_void_p
CF.CFDictionaryGetValue.argtypes=[ctypes.c_void_p,ctypes.c_void_p];CF.CFDictionaryGetValue.restype=ctypes.c_void_p
CF.CFNumberGetValue.argtypes=[ctypes.c_void_p,ctypes.c_int,ctypes.c_void_p];CF.CFNumberGetValue.restype=ctypes.c_bool
CF.CFRelease.argtypes=[ctypes.c_void_p]
CF.CFStringCreateWithCString.argtypes=[ctypes.c_void_p,ctypes.c_char_p,ctypes.c_uint32]
CF.CFStringCreateWithCString.restype=ctypes.c_void_p
OWNER=ctypes.c_void_p.in_dll(CG,'kCGWindowOwnerPID').value
BOUNDS=ctypes.c_void_p.in_dll(CG,'kCGWindowBounds').value
WIDTH=CF.CFStringCreateWithCString(None,b'Width',0x08000100)
HEIGHT=CF.CFStringCreateWithCString(None,b'Height',0x08000100)

def owned_on_screen_bounds(pid):
    # On-screen only, with no image capture or title access. A tray icon is too
    # small to qualify; these workloads create just one launcher window.
    windows=CG.CGWindowListCopyWindowInfo(1,0)
    if not windows:raise RuntimeError('CoreGraphics window metadata unavailable')
    result=[]
    try:
        for i in range(CF.CFArrayGetCount(windows)):
            window=CF.CFArrayGetValueAtIndex(windows,i)
            owner=CF.CFDictionaryGetValue(window,OWNER);value=ctypes.c_int64()
            if not owner or not CF.CFNumberGetValue(owner,4,ctypes.byref(value)) or value.value!=pid:continue
            bounds=CF.CFDictionaryGetValue(window,BOUNDS)
            width,height=ctypes.c_double(),ctypes.c_double()
            CF.CFNumberGetValue(CF.CFDictionaryGetValue(bounds,WIDTH),6,ctypes.byref(width))
            CF.CFNumberGetValue(CF.CFDictionaryGetValue(bounds,HEIGHT),6,ctypes.byref(height))
            result.append(dict(width=width.value,height=height.value))
        return result
    finally:CF.CFRelease(windows)

def main_on_screen(pid):
    return any(w['width']>=480 and w['height']>=80 for w in owned_on_screen_bounds(pid))
