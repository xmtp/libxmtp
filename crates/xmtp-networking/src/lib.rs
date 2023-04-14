pub mod proto_helper;

pub fn test_request() -> Result<u16, String> {
    let resp = reqwest::blocking::get("https://httpbin.org/ip").map_err(|e| format!("{}", e))?;
    // if resp is successful, return the body otherwise return "Error: {}" with response code
    if resp.status().is_success() {
        Ok(resp.status().as_u16())
    } else {
        Err(format!("{}", resp.status()))
    }
}

pub fn selftest() -> u16 {
    let resp = test_request();
    resp.unwrap_or(777)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let resp = selftest();
        // Assert 200
        assert_eq!(resp, 200);
    }
}
