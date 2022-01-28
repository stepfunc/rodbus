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
                'api/client/error',
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
