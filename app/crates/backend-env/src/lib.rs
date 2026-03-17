use std::env;

pub fn envsubst(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut output = String::with_capacity(input.len());
    let mut index = 0;

    while index < chars.len() {
        if chars[index] != '$' {
            output.push(chars[index]);
            index += 1;
            continue;
        }

        if let Some((name_start, name_end, next_index)) = parse_braced_var(&chars, index) {
            let name: String = chars[name_start..name_end].iter().collect();
            output.push_str(&env::var(name).unwrap_or_default());
            index = next_index;
            continue;
        }

        if let Some((name_start, name_end, next_index)) = parse_plain_var(&chars, index) {
            let name: String = chars[name_start..name_end].iter().collect();
            output.push_str(&env::var(name).unwrap_or_default());
            index = next_index;
            continue;
        }

        output.push('$');
        index += 1;
    }

    output
}

fn parse_braced_var(chars: &[char], start: usize) -> Option<(usize, usize, usize)> {
    if chars.get(start) != Some(&'$') || chars.get(start + 1) != Some(&'{') {
        return None;
    }

    let name_start = start + 2;
    let first = *chars.get(name_start)?;
    if !is_var_start(first) {
        return None;
    }

    let mut end = name_start + 1;
    while let Some(ch) = chars.get(end) {
        if is_var_continue(*ch) {
            end += 1;
            continue;
        }
        break;
    }

    if chars.get(end) != Some(&'}') {
        return None;
    }

    Some((name_start, end, end + 1))
}

fn parse_plain_var(chars: &[char], start: usize) -> Option<(usize, usize, usize)> {
    if chars.get(start) != Some(&'$') {
        return None;
    }

    let name_start = start + 1;
    let first = *chars.get(name_start)?;
    if !is_var_start(first) {
        return None;
    }

    let mut end = name_start + 1;
    while let Some(ch) = chars.get(end) {
        if is_var_continue(*ch) {
            end += 1;
            continue;
        }
        break;
    }

    Some((name_start, end, end))
}

fn is_var_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}

fn is_var_continue(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphanumeric()
}

#[cfg(test)]
mod tests {
    use super::envsubst;
    use std::env;
    use std::sync::{LazyLock, Mutex};

    static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    #[test]
    fn expands_plain_and_braced_variables() {
        let _guard = ENV_LOCK.lock().expect("env lock poisoned");
        unsafe {
            env::set_var("ENV_ONE", "alpha");
            env::set_var("ENV_TWO", "beta");
        }

        let result = envsubst("a=$ENV_ONE b=${ENV_TWO}");
        assert_eq!(result, "a=alpha b=beta");
    }

    #[test]
    fn replaces_missing_variables_with_empty_string() {
        let _guard = ENV_LOCK.lock().expect("env lock poisoned");
        unsafe {
            env::remove_var("ENV_MISSING");
        }

        let result = envsubst("before-$ENV_MISSING-after");
        assert_eq!(result, "before--after");
    }

    #[test]
    fn leaves_default_syntax_untouched() {
        let _guard = ENV_LOCK.lock().expect("env lock poisoned");
        unsafe {
            env::set_var("ENV_WITH_DEFAULT", "set");
        }

        let result = envsubst("${ENV_WITH_DEFAULT:-fallback} ${ENV_MISSING:-fallback}");
        assert_eq!(
            result,
            "${ENV_WITH_DEFAULT:-fallback} ${ENV_MISSING:-fallback}"
        );
    }

    #[test]
    fn keeps_invalid_tokens_literal() {
        let _guard = ENV_LOCK.lock().expect("env lock poisoned");
        let result = envsubst("$9 ${9BAD} $$VALUE");
        assert_eq!(result, "$9 ${9BAD} $");
    }

    #[test]
    fn keeps_trailing_dollar_literal() {
        let _guard = ENV_LOCK.lock().expect("env lock poisoned");
        let result = envsubst("value$");
        assert_eq!(result, "value$");
    }
}
