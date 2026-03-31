use quote::quote;

pub fn parser(
    _attr: proc_macro::TokenStream,
    body: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let input_fn = syn::parse_macro_input!(body as syn::ItemFn);

    let fn_attrs = &input_fn.attrs;
    let fn_vis = &input_fn.vis;
    let fn_sig = &input_fn.sig;
    let fn_block = &input_fn.block;

    let output = quote! {
        #(#fn_attrs)*
        #fn_vis #fn_sig {
            use ::tracing_subscriber::fmt;

            // Drop guard that sends logs to log_parser when dropped.
            // The tracing guard must be dropped BEFORE we read the captured logs.
            struct __ParseLogsGuard {
                writer: ::xmtp_common::TestWriter,
                tracing_guard: ::core::option::Option<::tracing::subscriber::DefaultGuard>,
            }

            impl Drop for __ParseLogsGuard {
                fn drop(&mut self) {
                    // Drop tracing guard first so logs are flushed
                    self.tracing_guard.take();

                    let captured_logs = self.writer.as_string();
                    if !captured_logs.is_empty() {
                        let log_file = ::std::env::temp_dir().join(format!(
                            "xmtp_parser_{}.log",
                            ::std::time::SystemTime::now()
                                .duration_since(::std::time::UNIX_EPOCH)
                                .map(|d| d.as_millis())
                                .unwrap_or(0)
                        ));
                        if let Err(e) = ::std::fs::write(&log_file, &captured_logs) {
                            ::tracing::error!("Failed to write logs to file: {}", e);
                        } else {
                            let _ = ::std::process::Command::new("cargo")
                                .arg("run")
                                .arg("--release")
                                .arg("-p")
                                .arg("log_parser")
                                .arg("--")
                                .arg(&log_file)
                                .status();

                            // Remove the tempfile when done
                            let _ = ::std::fs::remove_file(&log_file);
                        }
                    } else {
                        ::tracing::warn!("Logs are empty. Nothing to show in log_parser.");
                    }

                }
            }

            let __log_writer = ::xmtp_common::TestWriter::new();
            let __subscriber = fmt::Subscriber::builder()
                .with_writer(__log_writer.clone())
                .with_level(true)
                .with_ansi(false)
                .finish();
            let __tracing_guard = ::tracing::subscriber::set_default(__subscriber);

            // This guard will run log_parser on drop, even if the test returns early or panics
            let __parse_logs_guard = __ParseLogsGuard {
                writer: __log_writer,
                tracing_guard: ::core::option::Option::Some(__tracing_guard),
            };

            // Execute the original function body directly
            #fn_block
        }
    };

    proc_macro::TokenStream::from(output)
}
