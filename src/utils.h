//
// Created by Théo Monnom on 01/05/2022.
//

#ifndef LIVEKIT_NATIVE_UTILS_H
#define LIVEKIT_NATIVE_UTILS_H

#include <string>
#include <boost/regex.hpp>

namespace livekit {

    // TODO Do I need path + query ?
    struct URL {
        std::string protocol;
        std::string host;
        std::string port;
    };

    static URL ParseURL(const std::string &url) {
        boost::regex reg("(ws|wss)://([^:]*):?(\\d*)(.*)");
        boost::match_results<std::string::const_iterator> groups;

        if (boost::regex_search(url, groups, reg)) {
            return URL{
                    groups[1],
                    groups[2],
                    groups[3]
            };
        }

        throw std::runtime_error{"failed to parse url"};
    }
}

#endif //LIVEKIT_NATIVE_UTILS_H
