# HTMX + Llama.cpp Server

This project integrates HTMX with the Llama.cpp server to provide a seamless and efficient server-side rendering experience.

## Prerequisites

Ensure you have the [Llama.cpp](https://github.com/ggerganov/llama.cpp/) server set up on your machine.

## Starting the Llama.cpp Server

To start the Llama.cpp server, execute the following command:

```bash
.\llama-server -ngl 100 --port 9090 -m <model_file.gguf> --host 0.0.0.0
```

Replace `<model_file.gguf>` with the path to your model file.

## Running the Application

To run the application, use the following command:

```bash
cargo run -- --llama http://<llama.cpp_server_IP>:9090
```

Replace `<llama.cpp_server_IP>` with the IP address of your Llama.cpp server.

## Screenshot

Below is a screenshot of the application in action:

<img width="380" alt="Screenshot 2024-06-30 at 8 03 39 AM" src="https://github.com/richardanaya/htmx_llamacpp_server/assets/294042/0f49a056-7f42-4c87-90f5-8cff795ae9f9">
