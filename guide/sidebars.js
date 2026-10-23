// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

module.exports = {
  someSidebar: {
    About: [
        'about/guide',
        'about/modbus',
        'about/versioning',
        'about/license',
        'about/dependencies',
    ],
    Languages: [
        'languages/bindings',
        {
            Bindings: [
                'languages/c_bindings',
                'languages/cpp_bindings',
                'languages/java',
                'languages/c_sharp',
            ]
        }
    ],
    API: [
        'api/logging',
        'api/runtime',
        'api/tls',
        {
            Client: [
                'api/client/tcp_client',
                'api/client/rtu_client',
                'api/client/tls_client',
                'api/client/requests',
            ]
        },
        {
            Server: [
                'api/server/tcp_server',
                'api/server/rtu_server',
                'api/server/tls_server',
                'api/server/database',
                'api/server/write_handler',
            ]
        },
    ],
    Examples: [
        'examples/summary'
    ],
  },
};
