#!/bin/bash

# Check if nasm is installed
if ! command -v nasm &> /dev/null; then
    echo "nasm not found. Installing via Homebrew..."
    
    # Check if Homebrew is installed
    if ! command -v brew &> /dev/null; then
        echo "Homebrew is not installed. Please install Homebrew first."
        exit 1
    fi
    
    # Install nasm using Homebrew
    brew install nasm
    
    if [ $? -eq 0 ]; then
        echo "nasm successfully installed."
    else
        echo "Failed to install nasm."
        exit 1
    fi
else
    echo "nasm is already installed."
fi
