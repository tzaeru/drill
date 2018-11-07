use std::collections::HashMap;
use std::io::Read;

use yaml_rust::Yaml;
use colored::*;
use serde_json;
use time;

use hyper::client::{Client};
use hyper::Response;
use tokio_tls::TlsConnector;
use native_tls::TlsConnector as NativeTlsConnector;
use hyper::header::HeaderName;
use hyper::Method;
use hyper::Body;
use hyper::http;
use hyper::rt::Future;
use hyper_tls::HttpsConnector;
use hyper::rt::Stream;

use interpolator;

use actions::{Runnable, Report};

static USER_AGENT: &'static str = "drill";

#[derive(Clone)]
pub struct Request {
  name: String,
  url: String,
  time: f64,
  method: String,
  headers: HashMap<String, String>,
  pub body: Option<String>,
  pub with_item: Option<Yaml>,
  pub assign: Option<String>,
}

impl Request {
  pub fn is_that_you(item: &Yaml) -> bool{
    item["request"].as_hash().is_some()
  }

  pub fn new(item: &Yaml, with_item: Option<Yaml>) -> Request {
    let reference: Option<&str> = item["assign"].as_str();
    let body: Option<&str> = item["request"]["body"].as_str();
    let method;

    let mut headers = HashMap::new();

    if let Some(v) = item["request"]["method"].as_str() {
      method = v.to_string().to_uppercase();
    } else {
      method = "GET".to_string();
    }

    if let Some(hash) = item["request"]["headers"].as_hash() {
      for (key, val) in hash.iter() {
        if let Some(vs) = val.as_str() {
          headers.insert(key.as_str().unwrap().to_string(), vs.to_string());
        } else {
          panic!("{} Headers must be strings!!", "WARNING!".yellow().bold());
        }
      }
    }

    Request {
      name: item["name"].as_str().unwrap().to_string(),
      url: item["request"]["url"].as_str().unwrap().to_string(),
      time: 0.0,
      method: method,
      headers: headers,
      body: body.map(str::to_string),
      with_item: with_item,
      assign: reference.map(str::to_string),
    }
  }

  fn send_request(&self, context: &mut HashMap<String, Yaml>, responses: &mut HashMap<String, serde_json::Value>) -> (Response<Body>, f64) {
    //let ssl = NativeTlsClient::new().unwrap();
    //let native_connector = NativeTlsConnector::new().unwrap();
    //let connector = TlsConnector::from(native_connector);
    let https = HttpsConnector::new(4).expect("TLS initialization failed");
    let client = Client::builder().build::<_, Body>(https);

    let begin = time::precise_time_s();

    let interpolated_url;
    let interpolated_body;
    // Resolve the url
    {
      let interpolator = interpolator::Interpolator::new(context, responses);
      interpolated_url = interpolator.resolve(&self.url);
    }

    // Method
    let method = match self.method.to_uppercase().as_ref() {
      "GET" => Method::GET,
      "POST" => Method::POST,
      "PUT" => Method::PUT,
      "PATCH" => Method::PATCH,
      "DELETE" => Method::DELETE,
      _ => panic!("Unknown method '{}'", self.method),
    };

    // Body
    let mut request_builder = http::Request::builder();

    request_builder
      .uri(&interpolated_url)
      .method(method);

    // Headers
    request_builder.header("User-Agent", USER_AGENT.to_string());

    for (key, val) in self.headers.iter() {
      // Resolve the body
      let interpolator = interpolator::Interpolator::new(context, responses);
      let interpolated_header = interpolator.resolve(val).to_owned();

      request_builder.header(HeaderName::from_bytes(key.as_bytes()).unwrap(), interpolated_header.clone().as_bytes());
    }

    if let Some(cookie) = context.get("cookie") {
      request_builder.header("cookie", String::from(cookie.as_str().unwrap()));
    }

    let mut request;
    if let Some(body) = self.body.as_ref() {
      let interpolator = interpolator::Interpolator::new(context, responses);
      interpolated_body = interpolator.resolve(body);
      request = request_builder.body(Body::from(interpolated_body));
    } 
    else {
      request = request_builder.body(Body::from(""));
    }

    let response_result = client.request(request.unwrap()).wait().unwrap();

    /*if let Err(e) = response_result {
      panic!("Error connecting '{}': {:?}", interpolated_url, e);
    }*/

    let response = response_result;
    let duration_ms = (time::precise_time_s() - begin) * 1000.0;

    println!("{:width$} {} {} {}{}", self.name.green(), interpolated_url.blue().bold(), response.status().to_string().yellow(), duration_ms.round().to_string().cyan(), "ms".cyan(), width=25);

    (response, duration_ms)
  }
}

impl Runnable for Request {
  fn execute(&self, context: &mut HashMap<String, Yaml>, responses: &mut HashMap<String, serde_json::Value>, reports: &mut Vec<Report>) {
    if self.with_item.is_some() {
      context.insert("item".to_string(), self.with_item.clone().unwrap());
    }

    let (mut response, duration_ms) = self.send_request(context, responses);

    reports.push(Report { name: self.name.to_owned(), duration: duration_ms, status: response.status().as_u16() });

    /*if let Some(&SetCookie(ref cookies)) = response.headers.get::<SetCookie>() {
      if let Some(cookie) = cookies.iter().next() {
        let value = String::from(cookie.split(";").next().unwrap());
        context.insert("cookie".to_string(), Yaml::String(value));
      }
    }*/

    if let Some(ref key) = self.assign {
      let mut data = String::new();

      let value: serde_json::Value = serde_json::from_str(&data).unwrap();

      responses.insert(key.to_owned(), value);
    }
  }

}
