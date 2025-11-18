#Requires AutoHotkey >=2.0

InitInputSettings() {
    SetDefaultMouseSpeed 0  ; Instant mouse movements. Gotta go fast
    SendMode "Event"    ;   low level handler for precise timing
    SetKeyDelay 0, 0    ; no keystroke delay
}

InitUIVars() {
    global
    reset_position := Array(376, 30)  ;   Origin within the titlebar of `Account`
    ;   Offsets for rearranging elements
    collapse_offset := -160
    option_offset := 28  ; offset for y coordinate between options, if stop_toggle is on, etc

    /* Hex RBG color of boolean elements at a bright(TRUE) state.
    Deeply coupled with their coordinates. Trial run to ensure accurate reflection of state */
    checked_true := 0xADB5D3
    collapse_true := 0x13151A

    /* UI elements in order, categorized by layout header
    Format: Array(X coord, Y coord, option count)
    Windows Resolution : 1920x1080
    DPI scale : 125%
    Todo: scale XY with DPI */

    ;   Account
    logout_button := Array(696, 98)

    ;   Keyboard Shortcuts
    hotkey_button := Array(412, 186)
    hotkey_stop := Array(412, 216)
    stop_button := Array(225, 223)
    stop_toggle := PixelGetColor(stop_button[1], stop_button[2])

    ;   Recorder Customization
    location_button := Array(277, 324, 4)
    opacity_button := Array(702, 353)
    honk_button := Array(224, 384)
    honk_toggled := PixelGetColor(honk_button[1], honk_button[2])
    encoder_button := Array(293, 410, 3)
    settings_button := Array(456, 410)

    ; Upload Manager
    move_button := Array(651, 468)
    open_button := Array(706, 468)
    date_min_button := Array(258, 528)
    date_max_button := Array(404, 528)
    /* TODO: grid out calendar coordinates
    291-601 X
    638-747 Y */

    ; Upload Tracker
    collapse_button := Array(31, 681)
    collapse_toggled := PixelGetColor(collapse_button[1], collapse_button[2])
    unreliable_button := Array(27, 887)
    after_button := Array(27, 913)
    upload_button := Array(383, 946)
    FAQ_button := Array(32, 1002)
    logs_button := Array(76, 1002)
    logs_button := Array(76, 1002)
    website_button := Array(689, 1002)
    return
}

WinMinimize "A"
if WinExist("ahk_exe OWL Control.exe") {
    InitInputSettings()
    InitUIVars()
    WinWaitActive("ahk_exe OWL Control.exe")
    MouseClick "left", reset_position[1], reset_position[2]   ; Resets tabs to top of UI

    if (stop_toggle == checked_true) {
        y := stop_button[2] + option_offset
        MouseClick "left", 223, y
    }

    if (collapse_toggled == collapse_true)
        MouseClick "left", collapse_button[1], collapse_button[2]

    ; dont click logout, hotkey, stop
    buttons := [location_button, opacity_button, honk_button, encoder_button,
        unreliable_button, after_button, upload_button, settings_button, date_min_button, date_max_button, logs_button,
        open_button, move_button, website_button, FAQ_button, collapse_button]

    for coords in buttons {
        if (coords.Length = 3) {
            repeat := coords[3]
            loop repeat {
                MouseClick "left", coords[1], coords[2]
                y := (coords[2] + (A_Index) * option_offset) - collapse_offset
                MouseClick "left", coords[1], y
            }
        } else {
            y := coords[2] - collapse_offset
            MouseClick "left", coords[1], coords[2]
        }
    }
}

/*
legacy key tabbing, might be useful for autogenerating command
slower execution than clicking tho
pc_website := 5 ; Press count
pc_logs := 2
pc_faq := 3
pc_upload := 4
pc_after := 5
pc_reliable := 6
pc_tracker := 9
pc_date_max := 10
pc_date_min := 11
pc_move := 13
pc_open := 14
pc_settings :=15
pc_video :=16
pc_honk :=17
pc_opacity :=18
pc_slider :=19
pc_location :=20
pc_stop_toggle :=
press_count := pc_move
tc_logout := 1  ; tab count
tc_hotkey := 3
tab_count := tc_hotkey

Loop press_count {
Send "{Shift down}{Tab down}{Tab up}{Shift up}"
}
*/
