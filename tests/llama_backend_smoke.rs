use llama_cpp_2::llama_backend::LlamaBackend;

#[test]
fn llama_backend_initializes() {
    LlamaBackend::init().expect("llama.cpp backend must initialize during build/test");
}
