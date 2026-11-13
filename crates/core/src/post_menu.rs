//! Facebook's own "Actions for this post" menu, through the backend's
//! `fb post-menu`: Interested / Not interested (feed posts only), Save,
//! Snooze, Hide, Report and whatever those open next. Each step presses one
//! more of Facebook's own labels, so any language works.

/// Arguments for `fb post-menu`: open the post's menu, then press `path` in order.
pub fn args(post: &str, path: &[String]) -> Vec<String> {
    let mut args = vec!["post-menu".to_owned(), post.to_owned()];
    for label in path {
        args.push("--press".to_owned());
        args.push(label.clone());
    }
    args
}

#[derive(Debug, Default, PartialEq)]
pub struct Choice {
    pub label: String,
    pub detail: String,
    pub on: bool,
}

/// What shows after the last press.
#[derive(Debug, Default, PartialEq)]
pub struct Step {
    /// Worked on the post's feed card, where Interested / Not interested live.
    pub feed_card: bool,
    /// Nothing is left open: the last press finished the action.
    pub done: bool,
    pub title: String,
    pub text: Vec<String>,
    pub choices: Vec<Choice>,
    /// Facebook's own confirmations ("Post hidden").
    pub notes: Vec<String>,
}

pub fn parse(output: &str) -> Step {
    let mut step = Step::default();
    for line in output.lines() {
        let mut fields = line.split('\t');
        let field = |value: Option<&str>| value.unwrap_or("").trim().to_owned();
        match fields.next() {
            Some("from") => step.feed_card = fields.next() == Some("feed"),
            Some("done") => step.done = true,
            Some("title") => step.title = field(fields.next()),
            Some("text") => step.text.push(field(fields.next())),
            Some("note") => step.notes.push(field(fields.next())),
            Some("option") => step.choices.push(Choice {
                label: field(fields.next()),
                detail: field(fields.next()),
                on: fields.next() == Some("on"),
            }),
            _ => {}
        }
    }
    step.text.retain(|line| !line.is_empty());
    step.notes.retain(|line| !line.is_empty());
    step.choices.retain(|choice| !choice.label.is_empty());
    step
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_press_each_label_in_order() {
        let path = ["More options".to_owned(), "Snooze Ann for 30 days".to_owned()];
        assert_eq!(
            args("/ann/posts/1", &path),
            [
                "post-menu",
                "/ann/posts/1",
                "--press",
                "More options",
                "--press",
                "Snooze Ann for 30 days"
            ]
        );
    }

    #[test]
    fn reads_a_menu_step() {
        let raw = "from\tfeed\ntitle\tFeed story\noption\tInterested\tMore like this\t\noption\tNot interested\t\t\noption\tTurn on notifications\t\ton\n";
        let step = parse(raw);
        assert!(step.feed_card && !step.done);
        assert_eq!(step.title, "Feed story");
        assert_eq!(step.choices.len(), 3);
        assert_eq!(step.choices[0].detail, "More like this");
        assert!(step.choices[2].on);
    }

    #[test]
    fn reads_a_finished_step_from_the_post_page() {
        let raw = "feedcard\tcard 3 removed\nfrom\tpost\ndone\nnote\tPost hidden\n";
        let step = parse(raw);
        assert!(step.done && !step.feed_card);
        assert_eq!(step.notes, ["Post hidden"]);
    }
}
