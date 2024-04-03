//
//  File.swift
//  
//
//  Created by Alex Risch on 3/28/24.
//

import Foundation

struct Metadata: Codable {
    let title: String
    let description: String
    let imageUrl: String
}


class ProxyClient {
    var baseUrl: String

    init(baseUrl: String) {
        self.baseUrl = baseUrl
    }

    func readMetadata(url: String) async throws -> GetMetadataResponse {
        let encodedUrl = url.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) ?? ""
        let fullUrl = "\(self.baseUrl)?url=\(encodedUrl)"
        guard let url = URL(string: fullUrl) else {
            throw URLError(.badURL)
        }

        let (data, response) = try await URLSession.shared.data(from: url)
        guard let httpResponse = response as? HTTPURLResponse else {
            throw URLError(.badServerResponse)
        }

        guard httpResponse.statusCode == 200 else {
            throw FramesClientError.readMetadataFailed(message: "Failed to read metadata for \(url)", code: httpResponse.statusCode)
        }

        let decoder = JSONDecoder()
        let metadataResponse: GetMetadataResponse = try decoder.decode(GetMetadataResponse.self, from: data)
        return metadataResponse
    }

    func post(url: String, payload: Codable) async throws -> GetMetadataResponse {
        
        let encodedUrl = url.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) ?? ""
        let fullUrl = "\(self.baseUrl)?url=\(encodedUrl)"
        guard let url = URL(string: fullUrl) else {
            throw URLError(.badURL)
        }
        let encoder = JSONEncoder()
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try encoder.encode(payload)

        let (data, response) = try await URLSession.shared.data(for: request)
        guard let httpResponse = response as? HTTPURLResponse, httpResponse.statusCode == 200 else {
            throw URLError(.badServerResponse)
        }

        let decoder = JSONDecoder()
        let metadataResponse = try decoder.decode(GetMetadataResponse.self, from: data)
        return metadataResponse
    }

    func postRedirect(url: String, payload: Codable) async throws -> PostRedirectResponse {
        let encodedUrl = url.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) ?? ""
        let fullUrl = "\(self.baseUrl)redirect?url=\(encodedUrl)"
            guard let url = URL(string: fullUrl) else {
                throw URLError(.badURL)
            }

            var request = URLRequest(url: url)
            request.httpMethod = "POST"
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
            request.httpBody = try JSONSerialization.data(withJSONObject: payload)

            let (data, response) = try await URLSession.shared.data(for: request)
            guard let httpResponse = response as? HTTPURLResponse else {
                throw URLError(.badServerResponse)
            }

            guard httpResponse.statusCode == 200 else {
                throw FramesClientError.postFrameFailed(message: "Failed to post to frame \(url)", code: httpResponse.statusCode)
            }

            let decoder = JSONDecoder()
            let postRedirectResponse = try decoder.decode(PostRedirectResponse.self, from: data)
            return postRedirectResponse
    }

    func mediaUrl(url: String) -> String {
        let encodedUrl = url.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) ?? ""
        let result = "\(self.baseUrl)media?url=\(encodedUrl)"
        return result;
    }
}


