if [[ "$OSTYPE" == "darwin"* ]]; then
    ./build_macos.sh --arch arm64
    mkdir -p lib
    cp -r mac-arm64-release/* .
else
    ./build_linux.sh --arch x64
    mkdir -p lib
    cp -r linux-x64-release/* .
fi