use std::{convert::TryInto, net::Ipv4Addr};

use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct IpNtoa;

impl Function for IpNtoa {
    fn identifier(&self) -> &'static str {
        "ip_ntoa"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::INTEGER,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "Example",
            source: r#"ip_ntoa!(67305985)"#,
            result: Ok("1.2.3.4"),
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(IpNtoaFn { value }))
    }
}

#[derive(Debug, Clone)]
struct IpNtoaFn {
    value: Box<dyn Expression>,
}

impl Expression for IpNtoaFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let i: u32 = self
            .value
            .resolve(ctx)?
            .try_integer()?
            .try_into()
            .map_err(|_| String::from("cannot convert to bytes: integer does not fit in u32"))?;

        let ip = Ipv4Addr::from(u32::from_be(i));

        Ok(ip.to_string().into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().fallible().bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        ip_ntoa => IpNtoa;

        invalid {
            args: func_args![value: u32::MAX as i64 + 1],
            want: Err("cannot convert to bytes: integer does not fit in u32"),
            tdef: TypeDef::new().fallible().bytes(),
        }

        valid {
            args: func_args![value: 67305985],
            want: Ok(value!("1.2.3.4")),
            tdef: TypeDef::new().fallible().bytes(),
        }
    ];
}
