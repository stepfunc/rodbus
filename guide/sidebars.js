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
        {
            Client: [
                'api/client/requests',
                'api/client/error',
                'api/client/tcp_client',
            ]
        },
        {
            Server: [
                'api/server/tcp_server',
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
