from contextlib import contextmanager
from dataclasses import dataclass
from typing import Optional, Tuple, Union

import ctypes
from ctypes import Structure, c_ulonglong, c_long, c_uint, c_bool

import os
import time
import asyncio

from .keybinds import CODE_TO_KEY


class MouseMoveData(Structure):
    _fields_ = [("timestamp", c_ulonglong), ("dx", c_long), ("dy", c_long)]


class MouseButtonData(Structure):
    _fields_ = [("timestamp", c_ulonglong), ("button", c_uint), ("down", c_bool)]


class MouseScrollData(Structure):
    _fields_ = [("timestamp", c_ulonglong), ("scrollAmount", c_long)]


class KeyboardData(Structure):
    _fields_ = [("timestamp", c_ulonglong), ("keyCode", c_uint), ("down", c_bool)]


def keycode_to_key(key: int):
    try:
        return chr(key)
    except ValueError:
        return None


@contextmanager
def open_raw_input_dll(dll_path: str = "./rawinputlib.dll"):
    lib = RawInputLib(dll_path)
    lib.open()
    try:
        yield lib
    finally:
        lib.close()


class RawInputLib:
    def __init__(self, dll_path):
        self.dll_path = dll_path
        self._load_dll()
        self._set_types()

    def open(self) -> bool:
        """
        Get ready to read raw input, starts C process
        Returns bool indicating success
        """
        return self.cdll.initialize_raw_input()

    def close(self):
        """
        Close reading process and release handlers
        """
        self.cdll.cleanup_raw_input()

    def _load_dll(self):
        import os

        self.cdll = None
        try:
            self.cdll = ctypes.CDLL(self.dll_path)
        except OSError:
            for root, dirs, files in os.walk("."):
                if "rawinputlib.dll" in files:
                    dll_path = os.path.join(root, "rawinputlib.dll")
                    try:
                        self.cdll = ctypes.CDLL(dll_path)
                        self.dll_path = dll_path
                        break
                    except OSError:
                        continue
        finally:
            if self.cdll is None:
                raise FileNotFoundError(f"Could not find or load rawinputlib dll.")

    def _set_types(self):
        self.cdll.initialize_raw_input.restype = ctypes.c_int
        self.cdll.get_mouse_move_input.argtypes = [ctypes.POINTER(MouseMoveData)]
        self.cdll.get_mouse_move_input.restype = ctypes.c_int
        self.cdll.get_mouse_button_input.argtypes = [ctypes.POINTER(MouseButtonData)]
        self.cdll.get_mouse_button_input.restype = ctypes.c_int
        self.cdll.get_mouse_scroll_input.argtypes = [ctypes.POINTER(MouseScrollData)]
        self.cdll.get_mouse_scroll_input.restype = ctypes.c_int
        self.cdll.get_keyboard_input.argtypes = [ctypes.POINTER(KeyboardData)]
        self.cdll.get_keyboard_input.restype = ctypes.c_int
        self.cdll.cleanup_raw_input.restype = None


class MouseMoveInputReader:
    def __init__(self, rawinput_lib: RawInputLib):
        self.rawinput_lib = rawinput_lib

    def get(self) -> Optional[MouseMoveData]:
        data = MouseMoveData()
        if self.rawinput_lib.cdll.get_mouse_move_input(ctypes.byref(data)):
            return data


class MouseButtonInputReader:
    def __init__(self, rawinput_lib: RawInputLib):
        self.rawinput_lib = rawinput_lib

    def get(self) -> Optional[MouseButtonData]:
        data = MouseButtonData()
        if self.rawinput_lib.cdll.get_mouse_button_input(ctypes.byref(data)):
            return data


class MouseScrollInputReader:
    def __init__(self, rawinput_lib: RawInputLib):
        self.rawinput_lib = rawinput_lib

    def get(self) -> Optional[MouseScrollData]:
        data = MouseScrollData()
        if self.rawinput_lib.cdll.get_mouse_scroll_input(ctypes.byref(data)):
            return data


class KeyboardInputReader:
    def __init__(self, rawinput_lib: RawInputLib):
        self.rawinput_lib = rawinput_lib

    def get(self) -> Optional[KeyboardData]:
        data = KeyboardData()
        if self.rawinput_lib.cdll.get_keyboard_input(ctypes.byref(data)):
            return data


InputData = Union[MouseMoveData, MouseButtonData, MouseScrollData, KeyboardData]


class RawInputReader:
    def __init__(self, rawinput_lib):
        self.rawinput_lib = rawinput_lib
        self.inputs = [
            MouseMoveInputReader(rawinput_lib),
            MouseButtonInputReader(rawinput_lib),
            MouseScrollInputReader(rawinput_lib),
            KeyboardInputReader(rawinput_lib),
        ]

    async def run(self, polling_frequency, broadcast):
        while True:
            for input in self.inputs:
                data = input.get()
                if data is not None:
                    await broadcast(data)
                    break
            await asyncio.sleep(polling_frequency)


from ..constants import POLLS_PER_FRAME, FPS


class HotkeyManager:
    def __init__(self):
        self.callbacks = {}
        self.delay = 1.0 / FPS / POLLS_PER_FRAME

        self.tasks = set()

    async def on_keypress(self, keyboard_data: KeyboardData):
        if keyboard_data.down and keyboard_data.keyCode in self.callbacks:
            try:
                await self.call(keyboard_data.keyCode)
            except Exception as e:
                raise RuntimeError(
                    f"Error calling hotkey callback for keycode {keyboard_data.keyCode}"
                ) from e

    async def call(self, keycode):
        await self.callbacks[keycode]()

    def add_callback(self, keycode, fn):
        if not isinstance(keycode, int):
            keycode = keycode.upper()
            try:
                keycode = {k for k, v in CODE_TO_KEY.items() if v == keycode}.pop()
            except:
                raise ValueError(f"Key {keycode} not supported by hotkey manager.")
        self.callbacks[keycode] = fn
