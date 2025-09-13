#!/bin/bash
# setup.sh - Complete setup script for Rust Video Streamer

set -e

echo "🎬 Rust Video Streamer Setup Script"
echo "===================================="

# Function to check if command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Check for required system commands
echo "📋 Checking system requirements..."

# Check for Rust
if ! command_exists rustc; then
    echo "❌ Rust not found. Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source $HOME/.cargo/env
    echo "✅ Rust installed successfully"
else
    echo "✅ Rust found: $(rustc --version)"
fi

# Check for yt-dlp
if ! command_exists yt-dlp; then
    echo "⚠️  yt-dlp not found. Please install it:"
    echo "   Linux/macOS: pip install yt-dlp"
    echo "   Or visit: https://github.com/yt-dlp/yt-dlp"
    read -p "Continue anyway? (y/n): " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        exit 1
    fi
else
    echo "✅ yt-dlp found: $(yt-dlp --version)"
fi

# Detect OS and install dependencies
echo "🔧 Installing system dependencies..."

if [[ "$OSTYPE" == "linux-gnu"* ]]; then
    # Linux
    if command_exists apt-get; then
        # Ubuntu/Debian
        echo "📦 Installing Ubuntu/Debian dependencies..."
        sudo apt-get update
        sudo apt-get install -y \
            libavformat-dev libavcodec-dev libavutil-dev \
            libavfilter-dev libavdevice-dev libswscale-dev \
            libswresample-dev libsdl2-dev libasound2-dev \
            pkg-config build-essential
        echo "✅ Ubuntu/Debian dependencies installed"
    elif command_exists yum; then
        # Red Hat/CentOS/Fedora
        echo "📦 Installing Red Hat/Fedora dependencies..."
        sudo yum install -y \
            ffmpeg-devel SDL2-devel alsa-lib-devel \
            pkgconfig gcc
        echo "✅ Red Hat/Fedora dependencies installed"
    else
        echo "⚠️  Unknown Linux distribution. Please install:"
        echo "   - FFmpeg development libraries"
        echo "   - SDL2 development libraries"
        echo "   - ALSA development libraries"
    fi
elif [[ "$OSTYPE" == "darwin"* ]]; then
    # macOS
    echo "📦 Installing macOS dependencies..."
    if command_exists brew; then
        brew install ffmpeg sdl2
        echo "✅ macOS dependencies installed"
    else
        echo "⚠️  Homebrew not found. Please install:"
        echo "   1. Install Homebrew: /bin/bash -c \"\$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)\""
        echo "   2. Run: brew install ffmpeg sdl2"
    fi
elif [[ "$OSTYPE" == "msys" ]]; then
    # Windows
    echo "📦 Windows detected. Dependencies:"
    echo "   - Install yt-dlp from: https://github.com/yt-dlp/yt-dlp/releases"
    echo "   - FFmpeg and SDL2 will be handled by bundled features"
else
    echo "⚠️  Unknown OS: $OSTYPE"
fi

# Create project directory
echo "📁 Setting up project structure..."
PROJECT_NAME="rust-video-streamer"

if [ -d "$PROJECT_NAME" ]; then
    echo "⚠️  Directory $PROJECT_NAME already exists"
    read -p "Remove and recreate? (y/n): " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        rm -rf "$PROJECT_NAME"
    else
        echo "❌ Setup cancelled"
        exit 1
    fi
fi

cargo new "$PROJECT_NAME"
cd "$PROJECT_NAME"

echo "✅ Project structure created"

# Create source files message
echo "📄 Next steps:"
echo "   1. Copy the provided source files to src/:"
echo "      - main.rs"
echo "      - error.rs" 
echo "      - decoder.rs"
echo "      - renderer.rs"
echo "      - audio.rs"
echo "      - player.rs"
echo "      - mp4_parser.rs (from your existing code)"
echo "   2. Copy the provided Cargo.toml to project root"
echo "   3. Run: cargo build --release"
echo "   4. Run: cargo run --release"

echo ""
echo "🔧 Build commands:"
echo "   cargo build --release      # Build optimized binary"
echo "   cargo run --release        # Run the application"
echo "   RUST_LOG=debug cargo run   # Run with debug logging"

echo ""
echo "🎮 Usage:"
echo "   1. Run the application"
echo "   2. Enter a YouTube URL when prompted"
echo "   3. Use keyboard controls:"
echo "      - SPACE: Play/Pause"
echo "      - UP/DOWN: Volume"
echo "      - M: Mute"
echo "      - Q/ESC: Quit"

echo ""
echo "✅ Setup complete! The project is ready in: $(pwd)"
echo "📖 Read README.md for detailed instructions"