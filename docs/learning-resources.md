## Learning Resources

Interested in understanding the technology behind local AI inference?

The following open-source projects and educational resources provide a solid foundation for learning about large language models, inference optimization, model quantization, and GPU acceleration.

### Core Inference Technologies

| Resource | Description |
|---|---|
| [llama.cpp](https://github.com/ggml-org/llama.cpp) | High-performance local LLM inference engine supporting GGUF models, quantization, and multiple hardware backends. |
| [ggml](https://github.com/ggml-org/ggml) | Low-level tensor computation library powering llama.cpp, with a focus on efficient execution across different hardware architectures. |
| [vLLM](https://github.com/vllm-project/vllm) | High-throughput LLM serving engine featuring advanced memory management, continuous batching, and efficient request scheduling. |
| [FlashAttention](https://github.com/Dao-AILab/flash-attention) | Optimized attention algorithms designed to reduce memory traffic and improve GPU computation efficiency. |
| [Triton](https://github.com/triton-lang/triton) | GPU programming language and compiler for developing high-performance computational kernels. |
| [BitNet](https://github.com/microsoft/BitNet) | Research and implementation resources for efficient inference with extremely low-bit language models. |

### Educational Resources

| Resource | Description |
|---|---|
| [The Illustrated Transformer](https://jalammar.github.io/illustrated-transformer/) | Visual introduction to Transformer architectures and attention mechanisms. |
| [Hugging Face LLM Course](https://huggingface.co/learn/llm-course/chapter1/1) | Free educational materials covering language models, tokenization, Transformers, and the Hugging Face ecosystem. |
| [llama.cpp Build Guide](https://github.com/ggml-org/llama.cpp/blob/master/docs/build.md) | Official instructions for building llama.cpp with CPU, Metal, CUDA, and other backends. |
| [llama.cpp Server Documentation](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md) | Documentation for local inference serving, API integration, streaming, and concurrent requests. |
| [llama.cpp Performance Tips](https://github.com/ggml-org/llama.cpp/blob/master/docs/development/token_generation_performance_tips.md) | Practical guidance for investigating inference performance and hardware utilization. |

### Suggested Learning Path

1. **Fundamentals:** Understand Transformers, tokenization, attention, and autoregressive generation.
2. **Local Inference:** Build llama.cpp and run GGUF models locally.
3. **Model Optimization:** Explore quantization, memory management, and KV caching.
4. **Hardware Acceleration:** Study ggml backends, Apple Metal, and GPU computation.
5. **Inference Serving:** Learn API integration, streaming, batching, and request scheduling.
6. **Advanced Optimization:** Explore vLLM, FlashAttention, Triton, and low-bit model architectures.

These resources are provided for educational purposes. Their inclusion does not imply official affiliation with or endorsement by their respective maintainers.