The `datadog_logs` sink no longer requires  a semantic meaning input definition for `message` and `timestamp` fields.

While the Datadog logs intake does handle these fields if they are present, they aren't required.

The only impact is that configurations which enable the [Log Namespace](https://vector.dev/blog/log-namespacing/) feature and use a Source input to this sink which does not itself define a semantic meaning for `message` and `timestamp`, no longer need to manually set the semantic meaning for these two fields through a remap transform.

Existing configurations that utilize the Legacy namespace are unaffected, as are configurations using the Vector namespace where the input source has defined the `message` and `timestamp` semantic meanings.
