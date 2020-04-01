use nom::bytes::complete::{is_not, tag, take_until};
use nom::{multi::*, sequence::*};
use regex::Regex;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::{env, fs::File, io, path::Path, str::FromStr};

#[derive(Debug)]
struct Renderer {
    pub lines: Vec<String>,
    pub variables: HashMap<String, String>,
    pub labels: HashMap<String, usize>,
    pub index: usize,
}

impl Renderer {
    fn new() -> Renderer {
        Renderer {
            lines: Vec::new(),
            variables: HashMap::new(),
            labels: HashMap::new(),
            index: 0,
        }
    }

    fn processfile(&mut self, file: File) {
        let reader = BufReader::new(file);

        for (index, curline) in reader.lines().enumerate() {
            let text = curline.unwrap();
            self.lines.push(text.clone());

            if text == "" {
                continue;
            }

            match &text[0..1] {
                ":" => {
                    self.labels.insert(text[1..].to_string(), index);
                }
                "@" => {
                    match self.tokenize(self.lines[index].clone(), "=") {
                        Ok((l, _)) => self.variables.insert(l[1..].to_string(), String::from("0")),
                        Err(_) => continue,
                    };
                }
                _ => continue,
            }
        }
    }

    fn process_variables(&self, text: String) -> String {
        let mut s = text.clone();

        for item in parse_variables(text.clone()).iter() {
            if item != "" {
                let var = match self.variables.get(&item[..]) {
                    Some(v) => v,
                    None => {
                        panic!("Variable Missing. It must be created before the block using it.")
                    }
                };
                s = s.replace(&format!("@{}", &item[..]), var);
            }
        }
        s
    }

    fn process_expression(&self, text: String) -> bool {
        let (left, mid, right) = self.get_expression(text.clone());
        let mut isnan = false;

        let lvalue = match tinyexpr::interp(&left[..]) {
            Ok(v) => v,
            Err(_) => {
                isnan = true;
                0.0
            }
        };

        let rvalue = match tinyexpr::interp(&right[..]) {
            Ok(v) => v,
            Err(_) => {
                isnan = true;
                0.0
            }
        };

        match &mid[..] {
            "==" => {
                if isnan {
                    left == right
                } else {
                    lvalue == rvalue
                }
            }
            "!=" => {
                if isnan {
                    left != right
                } else {
                    lvalue != rvalue
                }
            }
            "<=" => {
                if isnan {
                    panic!("strings cant be compared with <=, line {}", self.index)
                } else {
                    lvalue <= rvalue
                }
            }
            ">=" => {
                if isnan {
                    panic!("strings cant be compared with >=, line {}", self.index)
                } else {
                    lvalue >= rvalue
                }
            }
            "<" => {
                if isnan {
                    panic!("strings cant be compared with <, line {}", self.index)
                } else {
                    lvalue < rvalue
                }
            }
            ">" => {
                if isnan {
                    panic!("strings cant be compared with >, line {}", self.index)
                } else {
                    lvalue > rvalue
                }
            }
            _ => panic!("No expression pattern found. line {}", self.index),
        }
    }

    fn get_expression(&self, text: String) -> (String, String, String) {
        let re = Regex::new(r"!=|==|<=|>=|<|>").unwrap();
        let mut mid = String::new();

        for part in re.captures_iter(&text[..]) {
            mid.push_str(&part[0]);
            break;
        }

        let arr: Vec<&str> = text.split(&mid[..]).collect();

        if arr.len() != 2 {
            panic!(
                "Expressions must containa a left side, right side and a operator. Line {}",
                self.index
            );
        }

        (String::from(arr[0]), mid, String::from(arr[1]))
    }

    fn tokenize(&self, line: String, pat: &str) -> Result<(String, String), String> {
        let arr: Vec<&str> = line.split(pat).collect();

        if arr.len() != 2 {
            return Err(format!(
                "The Token {} contained {} but should have only 2 at line {}.
            It should be seperated by {}",
                line,
                arr.len(),
                self.index,
                pat,
            ));
        }

        let mut iter = arr.iter();
        Ok((
            String::from(*iter.next().expect("expected 2 names, got 0")),
            String::from(*iter.next().expect("expected 2 names, got 1")),
        ))
    }

    fn iftokenize(
        &self,
        line: String,
        pat: &str,
    ) -> Result<(usize, String, String, String), String> {
        let arr: Vec<&str> = line.split(pat).collect();

        if arr.len() < 2 || arr.len() > 3 {
            return Err(format!(
                "The Token {} contained {} but should have 2 or 3 parts at line {}.
            It should be seperated by {}",
                line,
                arr.len(),
                self.index,
                pat,
            ));
        }

        let mut iter = arr.iter();
        Ok((
            arr.len(),
            String::from(*iter.next().expect("expected 2 names, got 0")),
            String::from(*iter.next().expect("expected 2 names, got 1")),
            String::from(*iter.next().unwrap_or(&"")),
        ))
    }
}

fn parse_variables(line: String) -> Vec<String> {
    let arr: nom::IResult<&str, Vec<&str>> = many0(preceded(
        take_until("@"),
        preceded(tag("@"), is_not(" \0+-<>=().!#:;^/\\@")),
    ))(&line[..]);

    match &arr {
        Ok(v) => {
            let mut ret = Vec::new();

            for item in v.1.iter() {
                ret.insert(0, item.to_string())
            }

            ret
        }
        _ => Vec::new(),
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let path = Path::new(&args[1]);
    let display = path.display();
    let mut story = Renderer::new();

    let file = match File::open(&path) {
        Err(why) => panic!("couldn't open {}: {}", display, why),
        Ok(file) => file,
    };

    story.processfile(file);

    while story.index < story.lines.len() {
        let text = &story.lines[story.index];

        if text == "" {
            story.index += 1;
            continue;
        }

        match &text[0..1] {
            "\n" | "\r" | ":" | "*" => {
                story.index += 1;
                continue;
            }
            "|" => {
                println!("");
                story.index += 1;
                continue;
            }
            // Process goto
            "#" => {
                let label_name = text.replace("#", "");
                match story.labels.get(&label_name) {
                    Some(v) => story.index = *v,
                    None => {
                        panic!("Goto {} Missing. line {}", label_name, story.index);
                    }
                };

                continue;
            }
            // Process IF statement
            "!" => {
                let (count, mut left, mid, right) = story
                    .iftokenize(story.lines[story.index].clone(), ":")
                    .unwrap();
                left.remove(0);
                left = story.process_variables(left);

                let mut exp = mid.trim();

                if !story.process_expression(left) {
                    if count == 3 {
                        exp = right.trim();
                    } else {
                        story.index += 1;
                        continue;
                    }
                }

                match &exp[0..1] {
                    "#" => {
                        let label = exp.replace("#", "");
                        let pos = match story.labels.get(&label) {
                            Some(v) => v,
                            None => {
                                panic!("Goto {} Missing. Found on line {}", label, story.index);
                            }
                        };

                        story.index = *pos;
                        continue;
                    }
                    "@" => {
                        let (l, r) = story.tokenize(exp.to_string(), "=").unwrap();

                        if !story.variables.contains_key(&l[1..]) {
                            panic!("A Variable must be initalized outside of a if statement before it can be used.
                            The Variable {} on line {} is not Initalized yet.", &l[1..], story.index);
                        }

                        let p = story.process_variables(r);

                        match tinyexpr::interp(&p[..]) {
                            Ok(v) => {
                                //update as variable
                                *story.variables.get_mut(&l[1..]).unwrap() = v.to_string();
                            }
                            Err(_) => {
                                //no calulations done becuase its a string so process as string.
                                *story.variables.get_mut(&l[1..]).unwrap() = p.clone();
                            }
                        };
                        story.index += 1;
                        continue;
                    }
                    _ => println!("{}", exp),
                }
            }
            // Process variables
            "@" => {
                match story.tokenize(story.lines[story.index].clone(), "=") {
                    Ok((l, r)) => {
                        let p = story.process_variables(r);
                        match tinyexpr::interp(&p[..]) {
                            Ok(v) => {
                                //update as variable
                                *story.variables.get_mut(&l[1..]).unwrap() = v.to_string();
                            }
                            Err(_) => {
                                //no calulations done becuase its a string so process as string.
                                *story.variables.get_mut(&l[1..]).unwrap() = p.clone();
                            }
                        };
                    }
                    Err(_) => {
                        println!(
                            "{}",
                            story.process_variables(story.lines[story.index].clone())
                        );
                    }
                };

                story.index += 1;
                continue;
            }
            // Process questions
            "?" => {
                let mut gotos: Vec<String> = Vec::new();
                let mut q = 0;

                while &story.lines[story.index][0..1] == "?" {
                    let (left, mut right) = story
                        .tokenize(story.lines[story.index].clone(), ":")
                        .unwrap();
                    right = right.replace("#", "");
                    gotos.push(right);
                    println!("{}. {}", q + 1, &left[1..]);
                    q += 1;
                    story.index += 1;
                }

                let mut input: usize = 0;

                while input < 1 || input > q {
                    let mut ret: String = String::new();

                    let b = match io::stdin().read_line(&mut ret) {
                        Ok(_) => true,
                        Err(_) => false,
                    };

                    if !b {
                        println!("You must enter a number between 1 and {}", q);
                        continue;
                    }

                    ret = ret.replace("\r\n", "");

                    if ret.chars().any(char::is_alphabetic) {
                        println!("You must enter a NUMBER between 1 and {}", q);
                        ret.clear();
                        continue;
                    }

                    input = match FromStr::from_str(&ret[..]) {
                        Ok(i) => i,
                        Err(_) => {
                            println!("You must enter a number between 1 and {}", q);
                            0
                        }
                    };

                    if input < 1 || input > q {
                        println!("You must enter a number between 1 and {}", q);
                    }
                }

                let label = gotos.get(input - 1).unwrap();
                match story.labels.get(label) {
                    Some(v) => story.index = *v,
                    None => {
                        panic!(
                            "Goto {} Missing. Found on Question near line {}.",
                            label, story.index
                        );
                    }
                };

                continue;
            }
            // Process inputs
            "^" => {
                let (left, right) = story
                    .tokenize(story.lines[story.index].clone(), ":")
                    .unwrap();
                let mut ret: String = String::new();

                if !story.variables.contains_key(&right[1..]) {
                    panic!("A Variable must be initalized outside of a Input statement before it can be used.
                    The Variable {} on line {} is not Initalized yet.", &right[1..], story.index);
                }

                match &left[1..2] {
                    "i" => {
                        let l = true;

                        while l {
                            println!("\n{}", &left[2..]);

                            let b = match io::stdin().read_line(&mut ret) {
                                Ok(_) => true,
                                Err(_) => false,
                            };

                            if !b {
                                println!("You must enter something.");
                                continue;
                            }

                            if ret.chars().any(char::is_alphabetic) {
                                println!("You may only enter in a Number. Please try again.");
                                ret.clear();
                                continue;
                            } else {
                                break;
                            }
                        }
                    }
                    "s" => {
                        println!("\n{}", &left[2..]);

                        let b = match io::stdin().read_line(&mut ret) {
                            Ok(_) => true,
                            Err(_) => false,
                        };

                        if !b {
                            println!("You must enter something.");
                            continue;
                        }
                    }
                    _ => panic!(
                        "Missing a i or s for input type at line {}. Example: ^i hows many?",
                        story.index
                    ),
                }

                ret = ret.replace("\r\n", "");
                *story.variables.get_mut(&right[1..]).unwrap() = ret.clone();
                story.index += 1;
                continue;
            }
            // Ignore Regular text so we can print it.
            _ => {
                println!(
                    "{}",
                    story.process_variables(story.lines[story.index].clone())
                );
                story.index += 1;
            }
        }
    }
}
