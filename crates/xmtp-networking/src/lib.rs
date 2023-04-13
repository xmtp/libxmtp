

pub fn test_request() -> Result<String, String> {
    let resp = reqwest::blocking::get("https://httpbin.org/ip").map_err(|e| format!("{}", e))?;
    // if resp is successful, return the body otherwise return "Error: {}" with response code
    if resp.status().is_success() {
        Ok(resp.text().map_err(|e| format!("{}", e))?)
    } else {
        Err(format!("{}", resp.status()))
    }
}

pub fn selftest() -> String {
    let resp = test_request();
    match resp {
        Ok(s) => s,
        Err(e) => format!("error: {}", e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let resp = selftest();
        // Assert "Error" is not in the response
        assert!(!resp.contains("error"));
    }
}
