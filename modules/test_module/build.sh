cargo build --target wasm32-wasip1 --release
cp ./target/wasm32-wasip1/release/test_module.wasm $HOME/.local/share/aurorashell/modules/
echo "copied ./target/wasm32-wasip1/release/test_module.wasm to $HOME/.local/share/aurorashell/modules/"
