use actix_cors::Cors;
use actix_files as fs;
use actix_web::client::Client;
use actix_web::dev::{ServiceRequest, ServiceResponse};
use actix_web::web::{BytesMut, Data};
use actix_web::{web, HttpMessage};
use actix_web::middleware::Logger;
use actix_web::{App, HttpResponse, HttpServer};

use futures::stream::StreamExt;

use env_logger::Env;

use url::Url;

struct AppState {
    client: Client,
    api_url: Url
}

async fn reverse_proxy(mut req: ServiceRequest) -> Result<ServiceResponse, actix_web::Error> {
    //read request body
    let mut ori_body = BytesMut::new();
    let mut stream = req.take_payload();
    while let Some(chunk) = stream.next().await {
        ori_body.extend_from_slice(&chunk?);
    }

    //get app data
    let app_state = if let Some(app_state) = req.app_data::<Data<AppState>>() {
        app_state
    } else {
        return Err(actix_web::Error::from(HttpResponse::InternalServerError()));
    };

    //build request url
    let mut new_url = app_state.api_url.clone();
    new_url.set_path(req.uri().path().trim_start_matches("/api"));
    new_url.set_query(req.uri().query());

    //send forwarded request
    let forwarded_req = app_state.client.request_from(new_url.as_str(), req.head());
    let forwarded_req = if let Some(addr) = req.head().peer_addr.clone() {
        forwarded_req.header("x-forwarded-for", format!("{}", addr.ip()))
    } else {
        forwarded_req
    };
    let forwarded_req = forwarded_req.set_header("User-Agent", "Android com.kuangxiangciweimao.novel");

    println!("proxy request: {:?}", forwarded_req);

    // let mut res = forwarded_req.send().await?;
    // 完全等价下面的 match
    let mut res = match forwarded_req.send().await {
        Ok(r) => {
            println!("proxy request error: {:?}", r);
            r
        }
        Err(e) => {
            println!("proxy response: {:?}", e);
            return Err(actix_web::Error::from(e))
        }
    };

    let mut client_resp = HttpResponse::build(res.status());
    // Remove `Connection` as per
    // https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Connection#Directives
    for (header_name, header_value) in res.headers().iter().filter(|(h, _)| *h != "connection") {
        client_resp.header(header_name.clone(), header_value.clone());
    }

    let body = res.body().await?;
    Ok(req.into_response(client_resp.body(body)))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // bug:
    // https://github.com/actix/actix-web/issues/1047
    //
    // let client = Client::default();
    // // Create request builder and send request
    // let res = client.get("http://app.hbooker.com")
    //     .header("User-Agent", "Actix-web")
    //     .send().await;                      // <- Send http request
    // let result = match res {
    //     Ok(_) => "Ok(_)".to_string(),
    //     Err(e) => e.to_string()
    // };
    // println!("{:?}", result);

    // return Ok(());

    let url = "localhost:8000";
    let target = "https://app.hbooker.com";

    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    println!("请在浏览器打开链接: http://127.0.0.1:8000");

    HttpServer::new(move || {
        App::new()
            .data(AppState {
                client: Client::new(),
                api_url: Url::parse(target).unwrap()
            })
            .wrap(Logger::default())
            .wrap(
                Cors::default()
                    .max_age(86400)
                    .allowed_origin(target)
                    .allow_any_header()
                    .allowed_methods(vec!["GET", "POST", "DELETE"])
                    .supports_credentials(),
            )
            .service(web::scope("/api").wrap_fn(|req, _srv| reverse_proxy(req)))
            .service(fs::Files::new("/", "./static").index_file("index.html"))
    })
    .bind(&url)?
    .run()
    .await
}
