use std::error;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VK_0, VK_9, VK_A, VK_LSHIFT, VK_NUMPAD0, VK_NUMPAD9, VK_RSHIFT, VK_Z,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, HC_ACTION, KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT,
    SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL,
    WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_RBUTTONDOWN, WM_RBUTTONUP,
    WM_SYSKEYDOWN, WM_SYSKEYUP, WindowFromPoint,
};
use windows::core::PCWSTR;

static mut TARGET_HWND: HWND = HWND(0 as *mut _);

fn is_alnum_vk(vk: u32) -> bool {
    // '0'..'9'
    (vk >= VK_0.0 as u32 && vk <= VK_9.0 as u32) ||
    // 'A'..'Z'
    (vk >= VK_A.0 as u32 && vk <= VK_Z.0 as u32) ||
    // Numpad 0..9
    (vk >= VK_NUMPAD0.0 as u32 && vk <= VK_NUMPAD9.0 as u32)
}

fn is_modifier_allowed(vk: u32) -> bool {
    // Allow shift so users can type uppercase letters if they want.
    vk == VK_LSHIFT.0 as u32 || vk == VK_RSHIFT.0 as u32
}

unsafe extern "system" fn ll_keyboard_proc(n_code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if n_code >= 0 && n_code as u32 == HC_ACTION {
        let kbd = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let vk = kbd.vkCode;

        let is_key_event = matches!(
            wparam.0 as u32,
            WM_KEYDOWN | WM_SYSKEYDOWN | WM_KEYUP | WM_SYSKEYUP
        );

        if is_key_event {
            let allowed = is_alnum_vk(vk) || is_modifier_allowed(vk);

            if !allowed {
                return LRESULT(1);
            }
        }
    }

    unsafe { CallNextHookEx(None, n_code, wparam, lparam) }
}

unsafe extern "system" fn ll_mouse_proc(n_code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if n_code >= 0 && n_code as u32 == HC_ACTION {
        let ms = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
        if ms.pt.y < 5 {
            return LRESULT(1);
        }

        if wparam.0 as u32 == WM_LBUTTONDOWN
            || wparam.0 as u32 == WM_LBUTTONUP
            || wparam.0 as u32 == WM_RBUTTONDOWN
            || wparam.0 as u32 == WM_RBUTTONUP
        {
            if unsafe { WindowFromPoint(ms.pt) != TARGET_HWND } {
                return LRESULT(1);
            }
        }
    }

    unsafe { CallNextHookEx(None, n_code, wparam, lparam) }
}

pub fn hook(target_hwnd: u32) -> Result<(), Box<dyn error::Error>> {
    unsafe { TARGET_HWND = HWND(target_hwnd as *mut _) };
    let module = unsafe { GetModuleHandleW(PCWSTR::null()) }?;
    let keyboard_hook = unsafe {
        SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(ll_keyboard_proc),
            Some(module.into()),
            0, // system-wide (all threads)
        )
    }?;

    if keyboard_hook.is_invalid() {
        eprintln!("Failed to install keyboard hook");
        return Ok(());
    }

    let mouse_hook = unsafe {
        SetWindowsHookExW(
            WH_MOUSE_LL,
            Some(ll_mouse_proc),
            Some(module.into()),
            0, // system-wide (all threads)
        )
    }?;

    if mouse_hook.is_invalid() {
        eprintln!("Failed to install mouse hook");
        unsafe { UnhookWindowsHookEx(keyboard_hook) }?;
        return Ok(());
    }

    let mut msg = MSG::default();
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        UnhookWindowsHookEx(mouse_hook)?;
        UnhookWindowsHookEx(keyboard_hook)?;
    }
    Ok(())
}
