"""
Constants used throughout the application
"""

# Recording settings
FPS = 60  # Frames per second for tracking
MIN_FOOTAGE = 30  # This many seconds needed before something is "worth saving"
MAX_FOOTAGE = 60 * 10  # How long should each individual clip be?
RECORDING_WIDTH = 640  # Video recording width
RECORDING_HEIGHT = 360  # Video recording height

# Keep in sync with src/services/constants.ts (for now!)
API_BASE_URL = "https://api.openworldlabs.ai"
# API_BASE_URL = "http://localhost:8000"

# File formats
ROOT_DIR = (
    "./data_dump/games/"  # User should be able to set this, but we will need to use it
)
