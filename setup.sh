#!/bin/bash

# OWL Control Setup Script
# This script sets up both the Python and Electron/Next.js components

set -e  # Exit on error

echo "OWL Control Setup Script"
echo "======================"

# Detect OS
OS="unknown"
if [[ "$OSTYPE" == "linux-gnu"* ]]; then
    OS="linux"
elif [[ "$OSTYPE" == "darwin"* ]]; then
    OS="mac"
elif [[ "$OSTYPE" == "msys" || "$OSTYPE" == "cygwin" || "$OSTYPE" == "win32" ]]; then
    OS="windows"
fi

echo "Detected OS: $OS"

# Check for required tools
echo ""
echo "Checking required tools..."

# Check Node.js and npm
if ! command -v node &> /dev/null; then
    echo "Error: Node.js is not installed. Please install Node.js from https://nodejs.org/"
    exit 1
else
    echo "✓ Node.js $(node --version) found"
fi

if ! command -v npm &> /dev/null; then
    echo "Error: npm is not installed. Please install npm."
    exit 1
else
    echo "✓ npm $(npm --version) found"
fi

# Check Python
if ! command -v python3 &> /dev/null; then
    echo "Error: Python 3 is not installed. Please install Python 3."
    exit 1
else
    echo "✓ Python $(python3 --version) found"
fi

# Setup Python environment
echo ""
echo "Setting up Python environment..."

# Check for uv and install if needed
PYTHON_TOOL=""
if command -v uv &> /dev/null; then
    echo "✓ uv found"
    PYTHON_TOOL="uv"
else
    echo "uv not found. Installing uv..."
    curl -LsSf https://astral.sh/uv/install.sh | sh
    export PATH="$HOME/.cargo/bin:$PATH"
    if command -v uv &> /dev/null; then
        echo "✓ uv installed successfully"
        PYTHON_TOOL="uv"
    else
        echo "Failed to install uv. Please install uv manually."
        exit 1
    fi
fi

# Install Python dependencies with uv
echo "Installing Python dependencies with $PYTHON_TOOL..."
# Create and activate virtual environment with uv
uv venv
if [[ "$OS" == "windows" ]]; then
    source .venv/Scripts/activate
else
    source .venv/bin/activate
fi

# Install dependencies with uv
# Since we have a pyproject.toml, we can install as a package
uv pip install -e .

# Setup Node.js/Electron environment
echo ""
echo "Setting up Node.js/Electron environment..."

# Install npm dependencies
echo "Installing npm dependencies..."
npm install

# Build the Electron app
echo "Building the Electron app..."
npm run build

# Platform-specific setup
echo ""
echo "Platform-specific setup..."

if [[ "$OS" == "mac" ]]; then
    echo "Setting up macOS-specific configurations..."
    echo "Note: You may need to grant accessibility permissions to the app in System Preferences > Security & Privacy > Privacy > Accessibility"
elif [[ "$OS" == "linux" ]]; then
    echo "Setting up Linux-specific configurations..."
    echo "Note: You may need to install additional system packages for Electron to work properly"
elif [[ "$OS" == "windows" ]]; then
    echo "Setting up Windows-specific configurations..."
fi

# Create necessary directories
echo ""
echo "Creating necessary directories..."
mkdir -p logs
mkdir -p data
mkdir -p dist

# Setup environment file
echo ""
echo "Setting up environment configuration..."
if [ ! -f ".env" ]; then
    echo "Creating .env file..."
    cat > .env << EOF
# Environment configuration for OWL Control
NODE_ENV=development
PYTHON_TOOL=$PYTHON_TOOL

# Add your configuration here
# OBS_WEBSOCKET_URL=ws://localhost:4444
# OBS_WEBSOCKET_PASSWORD=your_password
EOF
    echo "Created .env file. Please update it with your configuration."
else
    echo ".env file already exists"
fi

echo ""
echo "Setup complete!"
echo ""
echo "To run the application:"
echo "1. Activate the Python environment:"
if [[ "$OS" == "windows" ]]; then
    echo "   source .venv/Scripts/activate"
else
    echo "   source .venv/bin/activate"
fi
echo "2. Run the Python backend:"
echo "   python vg_control/main.py"
echo "3. In a separate terminal, run the Electron app:"
echo "   npm start"
echo ""
echo "For development mode:"
echo "   npm run dev"
echo ""
echo "To package the application:"
echo "   npm run package"