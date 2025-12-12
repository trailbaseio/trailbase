-- Bootstrap articles by inserting a dummy article for the admin user.
INSERT INTO articles (
    title,
    intro,
    tag,
    author,
    body,
    image
)
SELECT
    'TrailBase is Here 🎉',
    'A rigorously simple and blazingly fast application base 😉',
    'important,example',
    id,
    'TrailBase provides core functionality such restful APIs, file upload, auth, access control and a convenient admin dashboard out of the box.',
    '{"id":"40e8d2a2-b025-435e-9aa0-4cb6b895ab2a","filename":"image.png","content_type":"image/png","mime_type":"image/png"}'
FROM _user
WHERE email = 'editor@localhost';
