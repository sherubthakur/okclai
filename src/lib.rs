mod openai;

use anyhow::anyhow;
use futures::{Stream, StreamExt};
pub use openai::OpenAIWrapper;
use regex::Regex;
use std::pin::Pin;
use std::process::Command;
use termimad::{ask, Answer, MadSkin, Question};

pub struct Settings {
    stream: bool,
    explain: bool,
    ask_before_execution: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            stream: true,
            explain: true,
            ask_before_execution: true,
        }
    }
}

impl Settings {
    pub fn new(stream: bool, explain: bool, ask_before_execution: bool) -> Self {
        Self {
            stream,
            explain,
            ask_before_execution,
        }
    }
}

pub struct OkClai<'a> {
    openai: OpenAIWrapper<'a>,
    settings: Settings,
    skin: MadSkin,
}

impl<'a> OkClai<'a> {
    pub fn new(openai: OpenAIWrapper<'a>, settings: Settings) -> Self {
        Self {
            openai,
            settings,
            skin: MadSkin::default_dark(),
        }
    }

    pub async fn execute(&self, command_descripton: &str) -> anyhow::Result<()> {
        let response: anyhow::Result<String> = if self.settings.stream {
            let response_stream = Box::pin(
                self.openai
                    .get_streaming_response(&command_descripton)
                    .await?,
            );
            let response = self.print_and_extract_response(response_stream).await?;
            Ok(response)
        } else {
            let response = self.openai.get_response(&command_descripton).await?;
            if self.settings.explain {
                println!("{}", self.skin.term_text(&response));
            }
            Ok(response)
        };

        let command = self.extract_code_block(&response?)?;
        if self.settings.explain || self.settings.ask_before_execution {
            println!();
            println!(
                "{}",
                self.skin
                    .term_text(&format!("## Command to execute:\n`{}`", command))
            );
        }

        if self.should_execute()? {
            let result = self.execute_command(&command)?;
            if self.settings.explain {
                println!();
                println!("{}", self.skin.term_text("## Output"));
            }
            print!("{}", result);
        }

        Ok(())
    }

    fn should_execute(&self) -> anyhow::Result<bool> {
        let mut execute_command = true;
        if self.settings.ask_before_execution {
            let choice = ask!(&self.skin, "**Do you want to execute the command**", ('n') {
                ('y', "*Yes*, please") => {
                    Some("yes")
                }
                ('n', "*No*, thank you.") => {
                    None
                }
            });

            if let None = choice {
                execute_command = false;
            }
        }

        Ok(execute_command)
    }

    async fn print_and_extract_response(
        &self,
        mut stream: Pin<Box<impl Stream<Item = Result<String, anyhow::Error>>>>,
    ) -> anyhow::Result<String> {
        let mut response = String::new();
        while let Some(result_token) = stream.next().await {
            match result_token {
                Ok(token) => {
                    if self.settings.explain {
                        print!("{}", token);
                    }
                    response = format!("{}{}", response, token);
                }
                Err(e) => return Err(e),
            }
        }
        if self.settings.explain {
            println!();
        }
        Ok(response)
    }

    fn extract_code_block(&self, input: &str) -> anyhow::Result<String> {
        let re = Regex::new(r"```(?:\w+)?\n?(?P<code>[\s\S]*?)\n?```")
            .map_err(|e| anyhow!("Error creating regex: {}", e))?;

        if let Some(captures) = re.captures(input) {
            let code = captures
                .name("code")
                .ok_or_else(|| anyhow!("No code block found"))?;
            Ok(code.as_str().to_string())
        } else {
            Err(anyhow!("No code block found"))
        }
    }

    fn execute_command(&self, command: &str) -> anyhow::Result<String> {
        let output = Command::new("sh").arg("-c").arg(&command).output()?;

        if !output.status.success() {
            let error_message = String::from_utf8_lossy(&output.stderr).to_string();
            Err(anyhow::anyhow!(
                "Command failed: {}\nError message: {}",
                command,
                error_message
            ))
        } else {
            let success_message = String::from_utf8_lossy(&output.stdout).to_string();
            Ok(success_message)
        }
    }
}
