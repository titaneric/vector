use shared::aws_cloudwatch_logs_subscription::AwsCloudWatchLogsSubscriptionMessage;
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct ParseAwsCloudWatchLogSubscriptionMessage;

impl Function for ParseAwsCloudWatchLogSubscriptionMessage {
    fn identifier(&self) -> &'static str {
        "parse_aws_cloudwatch_log_subscription_message"
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "valid",
            // TODO: Fix `encode_json` hack. See comment in `result` below for more info.
            source: indoc! {r#"
                encode_json(parse_aws_cloudwatch_log_subscription_message!(s'{
                    "messageType": "DATA_MESSAGE",
                    "owner": "111111111111",
                    "logGroup": "test",
                    "logStream": "test",
                    "subscriptionFilters": [
                        "Destination"
                    ],
                    "logEvents": [
                        {
                            "id": "35683658089614582423604394983260738922885519999578275840",
                            "timestamp": 1600110569039,
                            "message": "{\"bytes\":26780,\"datetime\":\"14/Sep/2020:11:45:41-0400\",\"host\":\"157.130.216.193\",\"method\":\"PUT\",\"protocol\":\"HTTP/1.0\",\"referer\":\"https://www.principalcross-platform.io/markets/ubiquitous\",\"request\":\"/expedite/convergence\",\"source_type\":\"stdin\",\"status\":301,\"user-identifier\":\"-\"}"
                        }
                    ]
                }'))
            "#},
            result: Ok(
                // TODO: Remove string literal and use object literal instead.
                // When removing the string quotes, it doesn't seem to parse, but works fine in the CLI?
                // I boiled it down to timestamps in arrays failing to parse, e.g
                //     { "foo": [t'2021-02-02T19:41:00Z'] }
                // or
                //     { "foo": [{ "timestamp": 2021-02-02T19:41:00Z }] }
                r#"s'{"log_events":[{"id":"35683658089614582423604394983260738922885519999578275840","message":"{\"bytes\":26780,\"datetime\":\"14/Sep/2020:11:45:41-0400\",\"host\":\"157.130.216.193\",\"method\":\"PUT\",\"protocol\":\"HTTP/1.0\",\"referer\":\"https://www.principalcross-platform.io/markets/ubiquitous\",\"request\":\"/expedite/convergence\",\"source_type\":\"stdin\",\"status\":301,\"user-identifier\":\"-\"}","timestamp":"2020-09-14T19:09:29.039+00:00"}],"log_group":"test","log_stream":"test","message_type":"DATA_MESSAGE","owner":"111111111111","subscription_filters":["Destination"]}'"#,
            ),
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(ParseAwsCloudWatchLogSubscriptionMessageFn {
            value,
        }))
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::BYTES,
            required: true,
        }]
    }
}

#[derive(Debug, Clone)]
struct ParseAwsCloudWatchLogSubscriptionMessageFn {
    value: Box<dyn Expression>,
}

impl Expression for ParseAwsCloudWatchLogSubscriptionMessageFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let bytes = self.value.resolve(ctx)?.unwrap_bytes();

        let message = serde_json::from_slice::<AwsCloudWatchLogsSubscriptionMessage>(&bytes)
            .map_err(|e| format!("unable to parse: {}", e))?;

        Ok(map![
            "owner": message.owner,
            "message_type": message.message_type.as_str(),
            "log_group": message.log_group,
            "log_stream": message.log_stream,
            "subscription_filters": message.subscription_filters,
            "log_events": message.log_events.into_iter().map(|event| map![
                "id": event.id,
                "timestamp": event.timestamp,
                "message": event.message,
            ]).collect::<Vec<_>>(),
        ]
        .into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new()
            .fallible() // Message parsing error
            .object::<&str, TypeDef>(map! {
                "owner": Kind::Bytes,
                "message_type": Kind::Bytes,
                "log_group": Kind::Bytes,
                "log_stream": Kind::Bytes,
                "subscription_filters": TypeDef::new().array_mapped::<(), Kind>(map! {
                    (): Kind::Bytes
                }),
                "log_events": TypeDef::new().object::<&str, Kind>(map! {
                    "id": Kind::Bytes,
                    "timestamp": Kind::Timestamp,
                    "message": Kind::Bytes,
                }),
            })
    }
}

#[cfg(test)]
mod tests {
    //     use super::*;
    //     use chrono::{TimeZone, Utc};

    //     test_function![
    //         parse_aws_cloudwatch_log_subscription_message => ParseAwsCloudWatchLogSubscriptionMessage;

    //         invalid_type {
    //             args: func_args![value: "42"],
    //             want: Err("function call error: unable to parse: invalid type: integer `42`, expected struct AwsCloudWatchLogsSubscriptionMessage at line 1 column 2"),
    //         }

    //         string {
    //             args: func_args![value: r#"
    // {
    //     "messageType": "DATA_MESSAGE",
    //     "owner": "071959437513",
    //     "logGroup": "/jesse/test",
    //     "logStream": "test",
    //     "subscriptionFilters": [
    //     "Destination"
    //     ],
    //     "logEvents": [
    //     {
    //         "id": "35683658089614582423604394983260738922885519999578275840",
    //         "timestamp": 1600110569039,
    //         "message": "{\"bytes\":26780,\"datetime\":\"14/Sep/2020:11:45:41 -0400\",\"host\":\"157.130.216.193\",\"method\":\"PUT\",\"protocol\":\"HTTP/1.0\",\"referer\":\"https://www.principalcross-platform.io/markets/ubiquitous\",\"request\":\"/expedite/convergence\",\"source_type\":\"stdin\",\"status\":301,\"user-identifier\":\"-\"}"
    //     },
    //     {
    //         "id": "35683658089659183914001456229543810359430816722590236673",
    //         "timestamp": 1600110569041,
    //         "message": "{\"bytes\":17707,\"datetime\":\"14/Sep/2020:11:45:41 -0400\",\"host\":\"109.81.244.252\",\"method\":\"GET\",\"protocol\":\"HTTP/2.0\",\"referer\":\"http://www.investormission-critical.io/24/7/vortals\",\"request\":\"/scale/functionalities/optimize\",\"source_type\":\"stdin\",\"status\":502,\"user-identifier\":\"feeney1708\"}"
    //     }
    //     ]
    // }
    // "#],
    //             want: Ok(map![
    //                 "owner": "071959437513",
    //                 "message_type": "DATA_MESSAGE",
    //                 "log_group": "/jesse/test",
    //                 "log_stream": "test",
    //                 "subscription_filters": vec!["Destination"],
    //                 "log_events": vec![map![
    //                     "id": "35683658089614582423604394983260738922885519999578275840",
    //                     "timestamp": Utc.timestamp(1600110569, 39000000),
    //                     "message": "{\"bytes\":26780,\"datetime\":\"14/Sep/2020:11:45:41 -0400\",\"host\":\"157.130.216.193\",\"method\":\"PUT\",\"protocol\":\"HTTP/1.0\",\"referer\":\"https://www.principalcross-platform.io/markets/ubiquitous\",\"request\":\"/expedite/convergence\",\"source_type\":\"stdin\",\"status\":301,\"user-identifier\":\"-\"}",
    //                 ], map![
    //                     "id": "35683658089659183914001456229543810359430816722590236673",
    //                     "timestamp": Utc.timestamp(1600110569, 41000000),
    //                     "message": "{\"bytes\":17707,\"datetime\":\"14/Sep/2020:11:45:41 -0400\",\"host\":\"109.81.244.252\",\"method\":\"GET\",\"protocol\":\"HTTP/2.0\",\"referer\":\"http://www.investormission-critical.io/24/7/vortals\",\"request\":\"/scale/functionalities/optimize\",\"source_type\":\"stdin\",\"status\":502,\"user-identifier\":\"feeney1708\"}",
    //                 ]],
    //             ])
    //         }

    //         invalid_value {
    //             args: func_args![value: r#"{ INVALID }"#],
    //             want: Err("function call error: unable to parse: key must be a string at line 1 column 3"),
    //         }
    //     ];

    //     test_type_def![value_string {
    //         expr: |_| ParseAwsCloudWatchLogSubscriptionMessageFn {
    //             value: Literal::from("foo").boxed(),
    //         },
    //         def: TypeDef {
    //             fallible: true,
    //             kind: Kind::Map,
    //             ..Default::default()
    //         },
    //     }];
}
