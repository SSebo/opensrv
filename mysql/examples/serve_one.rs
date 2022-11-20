// Copyright 2021 Datafuse Labs.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! After running this, you should be able to run:
//!
//! ```console
//! $ echo "SELECT * FROM foo" | mysql -h 127.0.0.1 --table
//! ```

use std::io;
use tokio::io::AsyncWrite;

use opensrv_mysql::*;
use tokio::net::TcpListener;

struct Backend;

#[async_trait::async_trait]
impl<W: AsyncWrite + Send + Unpin> AsyncMysqlShim<W> for Backend {
    type Error = io::Error;

    async fn on_prepare<'a>(
        &'a mut self,
        _: &'a str,
        info: StatementMetaWriter<'a, W>,
    ) -> io::Result<()> {
        info.reply(42, &[], &[]).await
    }

    async fn on_execute<'a>(
        &'a mut self,
        _: u32,
        _: opensrv_mysql::ParamParser<'a>,
        results: QueryResultWriter<'a, W>,
    ) -> io::Result<()> {
        results.completed(OkResponse::default()).await
    }

    async fn on_close(&mut self, _: u32) {}

    async fn on_query<'a>(
        &'a mut self,
        sql: &'a str,
        results: QueryResultWriter<'a, W>,
    ) -> io::Result<()> {
        println!("execute sql {:?}", sql);
        results.start(&[]).await?.finish().await
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("0.0.0.0:3306").await?;

    loop {
        let (stream, _) = listener.accept().await?;
        let (mut r, mut w) = stream.into_split();

        tokio::spawn(async move {
            let (is_ssl, params) =
                AsyncMysqlIntermediary::init_before_ssl(&mut Backend, &mut r, &mut w)
                    .await
                    .unwrap();
            // match <Backend as opensrv_mysql::AsyncMysqlShim<W>>::tls_config(&Backend) {
            match Backend.tls_config() {
                Some(config) if is_ssl => {
                    let (r, w) = switch_to_tls(config.clone(), r, w).await.unwrap();
                    let _ = AsyncMysqlIntermediary::run_on(Backend, r, w).await;
                    // let reader = PacketReader::new(r);
                    // let writer = PacketWriter::new(w);
                    // let mi = AsyncMysqlIntermediary {
                    //     client_capabilities,
                    //     process_use_statement_on_query,
                    //     shim,
                    //     reader,
                    //     writer,
                    // };

                    // if let Some((handshake, seq, auth_context)) = params {
                    //     mi.init_after_ssl(handshake, seq, auth_context).await?;
                    // }
                    // mi.run().await
                }
                _ => {
                    let _ = AsyncMysqlIntermediary::run_on(Backend, r, w).await;
                }
            }
        });
    }
}
