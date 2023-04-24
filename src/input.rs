pub enum Input<'a> {
    Join {
        channel: &'a str,
    },
    Part {
        channel: &'a str,
    },
    Send {
        data: &'a str,
    },
    Usage {
        cmd: &'static str,
        message: &'static str,
    },
    Unknown {
        data: &'a str,
    },
}

impl<'a> Input<'a> {
    pub fn parse(input: &'a str) -> Self {
        let Some(tail) = input.strip_prefix('/') else {
            return Self::Send { data: input };
        };

        if let Some((head, tail)) = tail.split_once(' ') {
            match head {
                "join" | "enter" => {
                    if tail.is_empty() {
                        return Self::Usage {
                            cmd: "/join",
                            message: "syntax: /join channel",
                        };
                    }
                    return Self::Join { channel: tail };
                }
                "part" | "leave" => {
                    if tail.is_empty() {
                        return Self::Usage {
                            cmd: "/part",
                            message: "syntax: /part channel",
                        };
                    }
                    return Self::Part { channel: tail };
                }
                _ => {}
            }
        }

        Self::Unknown { data: input }
    }
}
