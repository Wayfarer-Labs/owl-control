import os
import pandas as pd
from datetime import datetime
import time
import json
import platform
import psutil
import logging

try:
    import GPUtil
except ImportError:
    GPUtil = None

try:
    from cpuinfo import get_cpu_info
except ImportError:
    get_cpu_info = None

try:
    import wmi
except ImportError:
    wmi = None

logger = logging.getLogger(__name__)


def get_hwid():
    try:
        with open("/sys/class/dmi/id/product_uuid", "r") as f:
            hardware_id = f.read().strip()
    except:
        try:
            # Fallback for Windows
            import subprocess

            output = subprocess.check_output("wmic csproduct get uuid").decode()
            hardware_id = output.split("\n")[1].strip()
        except:
            hardware_id = None
    return hardware_id


def get_cpu_specs():
    """Get CPU specifications"""
    try:
        cpu_info = {}

        # Use py-cpuinfo if available for detailed info
        if get_cpu_info:
            detailed_info = get_cpu_info()
            cpu_info["name"] = detailed_info.get("brand_raw", platform.processor())
            cpu_info["architecture"] = detailed_info.get("arch", platform.machine())
        else:
            cpu_info["name"] = platform.processor()
            cpu_info["architecture"] = platform.machine()

        # Use psutil for core/thread counts and frequency
        cpu_info["physical_cores"] = psutil.cpu_count(logical=False)
        cpu_info["logical_cores"] = psutil.cpu_count(logical=True)

        cpu_freq = psutil.cpu_freq()
        if cpu_freq:
            cpu_info["base_frequency_mhz"] = cpu_freq.current
            cpu_info["max_frequency_mhz"] = cpu_freq.max

        return cpu_info
    except Exception as e:
        logger.warning(f"Failed to get CPU specs: {e}")
        return {"name": "Unknown", "error": str(e)}


def get_gpu_specs():
    """Get GPU specifications"""
    gpu_info = []

    # Try GPUtil first (NVIDIA cards)
    if GPUtil:
        try:
            gpus = GPUtil.getGPUs()
            for gpu in gpus:
                gpu_info.append(
                    {
                        "name": gpu.name,
                        "memory_total_mb": gpu.memoryTotal,
                        "driver_version": gpu.driver,
                        "type": "NVIDIA",
                    }
                )
        except Exception as e:
            logger.debug(f"GPUtil failed: {e}")

    # Try WMI for all GPU types (Windows)
    if wmi and not gpu_info:
        try:
            c = wmi.WMI()
            for gpu in c.Win32_VideoController():
                if gpu.Name:
                    gpu_info.append(
                        {
                            "name": gpu.Name,
                            "memory_total_bytes": gpu.AdapterRAM,
                            "driver_version": gpu.DriverVersion,
                            "type": "WMI_Detected",
                        }
                    )
        except Exception as e:
            logger.debug(f"WMI GPU detection failed: {e}")

    if not gpu_info:
        gpu_info = [{"name": "Unknown", "error": "No GPU detection method available"}]

    return gpu_info


class Metadata:
    """
    When constructed, gets several pieces of information about the users setup.

    """

    def __init__(self, session_id):
        self.session_id = session_id
        self.data = {}
        self.path = None

        self.hardware_id = get_hwid()
        self.cpu_specs = get_cpu_specs()
        self.gpu_specs = get_gpu_specs()

    def reset(self, timestamp, path):
        self.data = {
            "session-id": self.session_id,
            "hardware-id": self.hardware_id,
            "start-timestamp": timestamp,
            "cpu_specs": self.cpu_specs,
            "gpu_specs": self.gpu_specs,
            "platform": {
                "system": platform.system(),
                "release": platform.release(),
                "version": platform.version(),
                "python_version": platform.python_version(),
            },
        }
        self.path = path
        self.start_time = time.time()

    def get_duration(self):
        return time.time() - self.start_time

    def end(self):
        timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
        self.data.update({"end-timestamp": timestamp, "duration": self.get_duration()})

        metadata_path = os.path.join(self.path, "metadata.json")

        with open(metadata_path, "w") as f:
            json.dump(self.data, f, indent=4)
