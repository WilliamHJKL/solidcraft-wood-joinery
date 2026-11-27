//! The plugin's messages in the language of its package.
//!
//! The messages are written in English, with their values in them ("the mortise leaves a 6.0 mm
//! cheek…"). A translated package carries templates: the English message with `{}` where a value
//! goes, and the same message in its language. A message that matches no template stays in
//! English. A value is translated in turn when it is a message itself, and a decimal number takes
//! the language's decimal separator.

/// One template: the English text cut at each `{}`, and the translation cut the same way.
pub struct Template {
    english: Vec<String>,
    translated: Vec<String>,
}

impl Template {
    pub fn new(english: &str, translated: &str) -> Self {
        Self { english: english.split("{}").map(str::to_owned).collect(), translated: translated.split("{}").map(str::to_owned).collect() }
    }

    /// The text of the template, without its values: the longest templates are tried first, so
    /// that a short one does not take a message a longer one describes better.
    fn fixed(&self) -> usize {
        self.english.iter().map(String::len).sum()
    }

    /// The values of `message`, when it is this template with values in place of its `{}`.
    fn values<'a>(&self, message: &'a str) -> Option<Vec<&'a str>> {
        let (first, rest) = self.english.split_first()?;
        let mut tail = message.strip_prefix(first.as_str())?;
        let mut values = Vec::new();
        for (i, part) in rest.iter().enumerate() {
            if i + 1 == rest.len() {
                // The last value runs up to the template's ending.
                let value = tail.strip_suffix(part.as_str())?;
                values.push(value);
                tail = "";
            } else {
                let at = tail.find(part.as_str()).filter(|_| !part.is_empty())?;
                values.push(&tail[..at]);
                tail = &tail[at + part.len()..];
            }
        }
        (tail.is_empty() && values.len() + 1 == self.english.len()).then_some(values)
    }
}

/// The templates of one language, and its decimal separator.
pub struct Language {
    templates: Vec<Template>,
    decimal: char,
}

impl Language {
    pub fn new(mut templates: Vec<Template>, decimal: char) -> Self {
        // Templates with values but too little fixed text ("{} {}") would take any message; a
        // text with no value ("made") is matched whole, whatever its length.
        templates.retain(|t| t.english.len() == 1 || t.fixed() >= 8);
        templates.sort_by_key(|t| std::cmp::Reverse(t.fixed()));
        Self { templates, decimal }
    }

    pub fn translate(&self, message: &str) -> String {
        self.translate_at(message, 0)
    }

    fn translate_at(&self, message: &str, depth: usize) -> String {
        if depth > 3 {
            return message.to_owned();
        }
        for template in &self.templates {
            let Some(values) = template.values(message) else { continue };
            if template.translated.len() != values.len() + 1 {
                continue;
            }
            let mut out = template.translated[0].clone();
            for (value, part) in values.iter().zip(&template.translated[1..]) {
                out.push_str(&self.value(value, depth));
                out.push_str(part);
            }
            return out;
        }
        message.to_owned()
    }

    fn value(&self, value: &str, depth: usize) -> String {
        let number = !value.is_empty() && value.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '-') && value.chars().any(|c| c.is_ascii_digit());
        if number {
            value.replace('.', &self.decimal.to_string())
        } else {
            self.translate_at(value, depth + 1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn french() -> Language {
        Language::new(
            vec![
                Template::new("the mortise leaves a {} mm cheek in the stile", "la mortaise laisse une joue de {} mm dans le montant"),
                Template::new("no joint matches; {}", "aucun assemblage ne correspond ; {}"),
                Template::new("the joints are: {}", "les assemblages sont : {}"),
                Template::new("{} {}", "{} {}"),
            ],
            ',',
        )
    }

    #[test]
    fn a_message_takes_its_translation_and_its_numbers_a_decimal_comma() {
        assert_eq!(french().translate("the mortise leaves a 6.0 mm cheek in the stile"), "la mortaise laisse une joue de 6,0 mm dans le montant");
    }

    #[test]
    fn a_value_that_is_a_message_is_translated_too() {
        assert_eq!(french().translate("no joint matches; the joints are: `A / B`"), "aucun assemblage ne correspond ; les assemblages sont : `A / B`");
    }

    #[test]
    fn a_message_without_a_template_stays_as_it_is() {
        assert_eq!(french().translate("something new"), "something new");
    }

    #[test]
    fn a_name_is_not_a_number() {
        assert_eq!(french().translate("no joint matches; Rail_2.5"), "aucun assemblage ne correspond ; Rail_2.5");
    }
}
