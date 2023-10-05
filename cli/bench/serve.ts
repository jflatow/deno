// Copyright 2018-2023 the Deno authors. All rights reserved. MIT license.

import { assert } from '../tests/unit/test_util.ts';

const s0 = Deno.serve({ port: 8080 }, function () {
  assert(arguments.length == 0);
  return new Response("hello world");
});

const s1 = Deno.serve({ port: 8081 }, function (_req) {
  assert(arguments.length == 1);
  return new Response("hello world");
});

const s2 = Deno.serve({ port: 8082 }, function (_req, _info) {
  assert(arguments.length == 2);
  return new Response("hello world");
});

let sum = 0;

Deno.bench(
  `0 handler args`,
  async () => {
    sum += (await (await fetch('http://localhost:8080')).text()).length;
  },
);

Deno.bench(
  `1 handler args`,
  async () => {
    sum += (await (await fetch('http://localhost:8081')).text()).length;
  },
);

Deno.bench(
  `2 handler args`,
  async () => {
    sum += (await (await fetch('http://localhost:8082')).text()).length;
  },
);